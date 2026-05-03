//! End-to-end test for `innisfree up`.
//!
//! Provisions a real DigitalOcean droplet, builds the wireguard tunnel,
//! runs a small HTTP server locally on port 8080, and asserts that fetching
//! the droplet's public IP returns the local server's response.
//!
//! Requires `DIGITALOCEAN_API_TOKEN` and `CAP_NET_ADMIN` on the binary. Use
//! `tools/test-runner` (or `just integration`) — it handles the setcap step
//! so the test process itself doesn't need to run as root.
//!
//! Gated behind the `integration` feature so `cargo test` doesn't pull it
//! in by default. Run with:
//!
//!     cargo test --features integration --test integration_test
//!
//! (Or just use `tools/test-runner`, which also handles the setcap step.)
#![cfg(feature = "integration")]

use anyhow::{anyhow, bail, Context, Result};
use rand::RngCore;
use std::env;
use std::future::Future;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

const LOCAL_PORT: u16 = 8080;
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(180);
const FETCH_TIMEOUT: Duration = Duration::from_secs(180);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test(flavor = "multi_thread")]
async fn simple_http_server_is_remotely_accessible() {
    // Fail fast: don't spin up everything just to discover later we can't
    // reach the cloud provider.
    if env::var_os("DIGITALOCEAN_API_TOKEN").is_none() {
        panic!("DIGITALOCEAN_API_TOKEN must be set");
    }

    let binary = env!("CARGO_BIN_EXE_innisfree");
    let test_string = make_test_string();

    let mut tunnel = Tunnel::up(binary)
        .await
        .expect("failed to spawn `innisfree up`");

    // Race the test body against the child's exit. If `innisfree up` dies
    // before the assertions complete (e.g. an API error during droplet
    // creation), surface that immediately instead of waiting out the
    // cloud-IP polling timeout.
    let outcome = tunnel
        .race_against_exit(run_checks(binary, &test_string))
        .await;

    // SIGINT triggers innisfree's own teardown (destroys the droplet); always
    // run it, even if the assertions above failed.
    let cleanup = tunnel.shutdown().await;

    if let Err(e) = outcome {
        panic!("integration test failed: {e:#}");
    }
    if let Err(e) = cleanup {
        panic!("tunnel cleanup failed: {e:#}");
    }
}

async fn run_checks(binary: &str, test_string: &str) -> Result<()> {
    // Bind locally before resolving the cloud IP so any early forwarded
    // connections find something listening.
    let _server = HttpServer::start(LOCAL_PORT, test_string)
        .await
        .context("starting local HTTP server")?;

    let control = http_get(&format!("http://127.0.0.1:{LOCAL_PORT}"))
        .await
        .context("control fetch from localhost failed")?;
    if control != test_string {
        bail!("control mismatch:\n  expected: {test_string:?}\n  got:      {control:?}");
    }

    let cloud_ip = wait_for_cloud_ip(binary, TUNNEL_TIMEOUT)
        .await
        .context("waiting for cloud IP to become available")?;

    // The droplet may need extra time after the IP appears for nginx and
    // wireguard to be fully wired up, so retry until success or deadline.
    let url = format!("http://{cloud_ip}:{LOCAL_PORT}");
    let body = fetch_with_retry(&url, FETCH_TIMEOUT)
        .await
        .context("fetching from cloud IP")?;
    if body != test_string {
        bail!("remote mismatch:\n  expected: {test_string:?}\n  got:      {body:?}");
    }
    Ok(())
}

fn make_test_string() -> String {
    let mut buf = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut buf);
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    format!("Hello, world! {hex}")
}

async fn http_get(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    Ok(resp.text().await?)
}

async fn fetch_with_retry(url: &str, total: Duration) -> Result<String> {
    let deadline = Instant::now() + total;
    let mut last: Option<anyhow::Error> = None;
    while Instant::now() < deadline {
        match http_get(url).await {
            Ok(body) => return Ok(body),
            Err(e) => {
                eprintln!("[integration] fetch retry: {e}");
                last = Some(e);
                sleep(Duration::from_secs(3)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| anyhow!("deadline reached without an attempt")))
}

async fn wait_for_cloud_ip(binary: &str, total: Duration) -> Result<String> {
    let deadline = Instant::now() + total;
    loop {
        let output = Command::new(binary).arg("ip").output().await?;
        if output.status.success() {
            let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ip.is_empty() {
                return Ok(ip);
            }
        }
        if Instant::now() >= deadline {
            bail!("timed out after {total:?} waiting for `innisfree ip` to return an address");
        }
        sleep(Duration::from_secs(2)).await;
    }
}

// ---------------------------------------------------------------------------
// background processes
// ---------------------------------------------------------------------------

struct Tunnel {
    child: Child,
}

impl Tunnel {
    async fn up(binary: &str) -> Result<Self> {
        let child = Command::new(binary)
            .args(["up", "-p", &LOCAL_PORT.to_string()])
            .env("RUST_LOG", "innisfree=trace")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawning `innisfree up`")?;
        Ok(Self { child })
    }

    /// Run `fut` to completion, but return early with a descriptive error if
    /// the spawned `innisfree` process exits before `fut` finishes. Without
    /// this, an early failure (bad API call, missing capability, panic) would
    /// surface only as a downstream timeout — the binary's own error message
    /// is on stderr, but the test wouldn't react to it.
    async fn race_against_exit<F, T>(&mut self, fut: F) -> Result<T>
    where
        F: Future<Output = Result<T>>,
    {
        tokio::select! {
            // Prefer the child-exit branch when both are ready, so a fast
            // failure isn't masked by a checks-result race.
            biased;
            wait_res = self.child.wait() => {
                let status = wait_res.context("waiting for innisfree")?;
                Err(anyhow!(
                    "`innisfree up` exited prematurely with status {status} \
                     before the test could finish (see stderr above for the \
                     underlying error)"
                ))
            }
            result = fut => result,
        }
    }

    async fn shutdown(mut self) -> Result<()> {
        // The race may have already reaped the child; nothing to do then.
        if let Some(_status) = self.child.try_wait().context("polling child status")? {
            return Ok(());
        }
        send_sigint(&self.child).context("sending SIGINT to innisfree")?;
        match timeout(SHUTDOWN_TIMEOUT, self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(e).context("waiting for innisfree to exit"),
            Err(_) => {
                let _ = self.child.start_kill();
                bail!(
                    "innisfree did not exit within {SHUTDOWN_TIMEOUT:?} after SIGINT; \
                     cloud droplet may need manual cleanup"
                );
            }
        }
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // Fallback for the test-panic path: send SIGINT and let innisfree
        // continue running after we exit so it can finish destroying the
        // droplet on its own. Drop is sync; we can't await here.
        let _ = send_sigint(&self.child);
    }
}

fn send_sigint(child: &Child) -> Result<()> {
    let pid = child.id().ok_or_else(|| anyhow!("child PID unavailable"))?;
    let pid: i32 = pid.try_into().context("PID overflow")?;
    let rc = unsafe { libc::kill(pid, libc::SIGINT) };
    if rc != 0 {
        return Err(anyhow::Error::from(std::io::Error::last_os_error()).context("kill(SIGINT)"));
    }
    Ok(())
}

struct HttpServer {
    handle: JoinHandle<()>,
}

impl HttpServer {
    /// Bind `0.0.0.0:port` and reply to every request with `body`. Stand-in
    /// for the original `python3 -m http.server`, but only serves one body.
    async fn start(port: u16, body: &str) -> Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))
            .await
            .with_context(|| format!("binding 0.0.0.0:{port}"))?;
        let body = body.to_string();
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(serve(stream, body.clone()));
                    }
                    Err(e) => {
                        eprintln!("[integration] accept error: {e}");
                        break;
                    }
                }
            }
        });
        Ok(Self { handle })
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve(mut stream: TcpStream, body: String) {
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf).await;
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Length: {}\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
