//! High-level controller logic for managing
//! service proxies, i.e. [TunnelManager].

use crate::config::{clean_config_dir, make_config_dir, ServicePort};

use crate::proxy::proxy_handler;
use crate::server::digitalocean::server::Droplet;
use crate::server::InnisfreeServer;
use crate::ssh::SshKeypair;
use crate::wg::{LocalWg, WireguardManager};
use anyhow::{anyhow, bail, Context, Result};
use futures::future::join_all;
use std::net::{IpAddr, SocketAddr, TcpStream};
use tokio::signal;

/// Controller class for handling tunnel configurations.
/// Handles the soup-to-nuts configuration, including server creation,
/// WireGuard device config, and proxy.
pub struct TunnelManager {
    /// List of `ServicePort`s to manage connections for.
    pub services: Vec<ServicePort>,
    // dest_ip: IpAddr,
    /// Remote server handling public ingress.
    pub server: Box<dyn InnisfreeServer>,
    /// Human-readable name for this service manager.
    pub name: String,
    /// Controller for Wireguard tunnels.
    pub wg: WireguardManager,
    /// SSH keypair for managing client-side SSH connections.
    pub ssh_client_keypair: SshKeypair,
    /// SSH keypair for identifying remote SSH server identity.
    pub ssh_server_keypair: SshKeypair,
    /// Live local Wireguard interface, populated by [`Self::up`].
    /// Dropping it tears the interface down.
    local_wg: Option<LocalWg>,
}

impl TunnelManager {
    /// Create a new controller for managing a collection of services.
    /// Call `up()` to build.
    pub async fn new(
        tunnel_name: &str,
        services: Vec<ServicePort>,
        static_ip: Option<IpAddr>,
    ) -> Result<TunnelManager> {
        clean_config_dir(tunnel_name)?;
        let wg = WireguardManager::new(tunnel_name)?;
        // Create new ephemeral ssh keypair
        let ssh_client_keypair = SshKeypair::new("client")?;
        let ssh_server_keypair = SshKeypair::new("server")?;
        let server = Droplet::new(
            tunnel_name,
            services.clone(),
            wg.clone(),
            &ssh_client_keypair,
            &ssh_server_keypair,
        )
        .await?;

        if let Some(ip) = static_ip {
            server.assign_floating_ip(ip).await?;
        }

        Ok(TunnelManager {
            name: tunnel_name.to_owned(),
            services,
            server: Box::new(server),
            ssh_client_keypair,
            ssh_server_keypair,
            wg,
            local_wg: None,
        })
    }
    /// Create remote and local infrastructure. Creates a cloud server,
    /// configures it to forward public ports over its Wireguard interface,
    /// to a local Wireguard interface
    pub async fn up(&mut self) -> Result<()> {
        self.wait_for_ssh()?;
        tracing::debug!("Configuring remote proxy...");
        self.wait_for_cloudinit()
            .context("failed while waiting for cloudinit")?;
        let ip = self.server.ipv4_address()?;
        tracing::debug!("Configuring tunnel...");
        // Stamp the freshly-discovered remote IP onto the local
        // device's peer config so the boringtun runtime can connect.
        self.wg.wg_local_device.peer.endpoint = Some(ip);
        // Persist the rendered wg0.conf for debugging / SSH inspection;
        // the local runtime no longer reads it, but it remains useful.
        self.wg
            .wg_local_device
            .write_locally(&self.name, &self.services)
            .context("failed to write wireguard configs")?;
        tracing::debug!("Bringing up remote Wireguard interface");
        self.bring_up_remote_wg()
            .context("failed to bring up remote wg interface")?;
        tracing::debug!("Bringing up local Wireguard interface");
        let local_wg = LocalWg::start(&self.wg.wg_local_device)
            .await
            .context("failed to bring up local wg interface")?;
        self.local_wg = Some(local_wg);

        tracing::trace!("Testing connection");
        self.test_connection()?;
        // The `ip` marker is the readiness signal for `innisfree ip` (and
        // anything polling it, like the integration test). Writing it here
        // — only after `test_connection` confirms the wg tunnel pings end
        // to end — guarantees a successful `innisfree ip` means the tunnel
        // is actually usable, not just that a known_hosts file got written
        // as a side effect during the cloud-init wait.
        self.write_ready_marker()
            .context("writing ready marker after tunnel came up")?;
        Ok(())
    }

    /// Persist the public IPv4 address of the cloud node to a small file
    /// in the per-tunnel config dir. Used by [`get_server_ip`] (and thus
    /// `innisfree ip` / `innisfree ssh`) as both the IP source and the
    /// readiness signal.
    fn write_ready_marker(&self) -> Result<()> {
        let ip = self.server.ipv4_address()?;
        let fpath = make_config_dir(&self.name)?.join("ip");
        std::fs::write(&fpath, format!("{ip}\n"))
            .with_context(|| format!("writing ready marker {}", fpath.display()))?;
        Ok(())
    }
    /// Blocks until the server's cloudinit process reports completion.
    fn wait_for_cloudinit(&self) -> Result<()> {
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_ssh_cmd(cmd)
    }
    /// Blocks until 22/TCP is available on the server.
    fn wait_for_ssh(&self) -> Result<()> {
        let dest_ip = SocketAddr::new(self.server.ipv4_address()?, 22);
        loop {
            let stream = TcpStream::connect(dest_ip);
            match stream {
                Ok(_) => {
                    tracing::debug!("SSH port is open, proceeding");
                    break;
                }
                Err(_) => {
                    tracing::debug!("Waiting for ssh...");
                    tracing::trace!("Polling socket {})...", dest_ip);
                    std::thread::sleep(std::time::Duration::from_secs(10));
                }
            }
        }
        Ok(())
    }
    /// Wait for an interrupt signal, then terminate gracefully,
    /// cleaning up droplet resources before exit.
    pub async fn block(&mut self) -> Result<()> {
        match signal::ctrl_c().await {
            Ok(()) => {
                tracing::warn!("Received stop signal, exiting gracefully");
                self.clean().await?;
                tracing::info!("Clean up complete, exiting");
                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!("Unable to register hook for ctrl+c: {}", e);
                std::process::exit(10);
            }
        }
    }
    /// Ping remote remote Wireguard IP from local Wireguard device.
    /// Ensures connectivity is established between remote and local interfaces.
    fn test_connection(&self) -> Result<()> {
        tracing::trace!("Inside test connection, setting up vars");
        let ip = &self.wg.wg_remote_ip;
        tracing::trace!("Inside test connection, running ping cmd");
        std::process::Command::new("ping")
            .arg("-c1")
            .arg("-w5")
            .arg(ip.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Failed to ping remote Wireguard interface, tunnel broken")?;
        tracing::debug!("Confirmed tunnel is established, able to ping across it");
        Ok(())
    }
    /// Runs `wg-quick up` on remote server to bring up its Wireguard interface.
    /// Remote node is still Debian + cloud-init + wg-quick (Phase 2 will
    /// migrate it to NixOS); local side now uses the in-process runtime.
    fn bring_up_remote_wg(&self) -> Result<()> {
        let cmd = vec!["wg-quick", "up", "/tmp/innisfree.conf"];
        tracing::trace!("Activating remote wg interface");
        self.run_ssh_cmd(cmd)
    }
    /// Generates an SSH known_hosts file, containing the automatically
    /// generated SSH hostkey for the remote server. Doing so allows
    /// us to verify the SSH connection on first use.
    fn known_hosts(&self) -> Result<String> {
        let ipv4_address = &self.server.ipv4_address()?;
        let server_host_key = &self.ssh_server_keypair.public;
        let host_line = format!("{} {}", ipv4_address, server_host_key);
        let fpath = make_config_dir(&self.name)?.join("known_hosts");
        std::fs::write(&fpath, host_line).context("Failed to create known_hosts")?;
        Ok(fpath.display().to_string())
    }
    /// Execute a shell command on the remote server. Fails if the remote
    /// command exits non-zero, surfacing its captured stderr/stdout so the
    /// caller can diagnose what went wrong (`Command::status()` reports the
    /// exit code as `Ok(_)`, so the previous Stdio::null() + `?` shape was
    /// silently swallowing remote-side failures).
    fn run_ssh_cmd(&self, cmd: Vec<&str>) -> Result<()> {
        tracing::trace!("Entering run_ssh_cmd");
        let ssh_kp = &self.ssh_client_keypair.write_locally(&self.name)?;
        let ssh_kp_s = ssh_kp.display().to_string();
        let known_hosts_opt = format!("UserKnownHostsFile={}", &self.known_hosts()?);
        let ipv4_address = &self.server.ipv4_address()?.to_string();
        let pretty_cmd = cmd.join(" ");
        let mut cmd_args = vec![
            "-l",
            "innisfree",
            "-i",
            &ssh_kp_s,
            "-o",
            &known_hosts_opt,
            "-o",
            "ConnectTimeout=5",
            ipv4_address,
        ];
        cmd_args.extend(cmd);
        let output = std::process::Command::new("ssh")
            .args(cmd_args)
            .output()
            .context("invoking ssh")?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            bail!(
                "remote command `{pretty_cmd}` failed (ssh exit {}):\n\
                 --- stderr ---\n{}\n--- stdout ---\n{}",
                output.status,
                stderr.trim_end(),
                stdout.trim_end(),
            );
        }
        if !stdout.trim().is_empty() {
            tracing::debug!("remote `{pretty_cmd}` stdout:\n{}", stdout.trim_end());
        }
        if !stderr.trim().is_empty() {
            tracing::debug!("remote `{pretty_cmd}` stderr:\n{}", stderr.trim_end());
        }
        Ok(())
    }
    /// Destroys all infrastructure, including local Wireguard interfaces,
    /// remote server, and local config dir.
    pub async fn clean(&mut self) -> Result<()> {
        tracing::debug!("removing local Wireguard interface");
        // Drop the runtime: boringtun's DeviceHandle::Drop tears down
        // the TUN device and joins worker threads.
        self.local_wg = None;

        // Run both cleanup steps regardless of which fails: a leaked droplet
        // costs money, but local config is also worth removing. Capture each
        // result, log loudly on a destroy failure (so the user knows manual
        // cleanup is needed), then propagate.
        let destroy_result = self
            .server
            .destroy()
            .await
            .context("destroying remote server");
        if let Err(e) = &destroy_result {
            tracing::error!(
                "failed to destroy remote server (manual cleanup may be required): {e:#}"
            );
        }
        let config_result = clean_config_dir(&self.name).context("removing local config dir");

        destroy_result?;
        config_result
    }
}

/// Look up the IPv4 address for the remote server. The marker file is
/// written by [`TunnelManager::write_ready_marker`] only after the tunnel
/// is verified up, so a successful return from this function doubles as
/// "the tunnel is ready" — callers polling for readiness don't need a
/// separate signal.
pub fn get_server_ip(service_name: &str) -> Result<IpAddr> {
    tracing::trace!("Looking up server IP from ready marker");
    let fpath = make_config_dir(service_name)?.join("ip");
    let content = std::fs::read_to_string(&fpath)
        .with_context(|| format!("reading {} (tunnel may not be ready yet)", fpath.display()))?;
    let ip: IpAddr = content
        .trim()
        .parse()
        .with_context(|| format!("parsing IP from {}", fpath.display()))?;
    Ok(ip)
}

/// Create an interface SSH session on remote server.
pub fn open_shell(service_name: &str) -> Result<()> {
    let client_key = make_config_dir(service_name)?.join("client_id_ed25519");
    let client_key_s = client_key.display().to_string();
    let known_hosts = make_config_dir(service_name)?.join("known_hosts");
    let known_hosts_opt = format!("UserKnownHostsFile={}", known_hosts.display());
    let ipv4_address = get_server_ip(service_name)?.to_string();
    let cmd_args = vec![
        "-l",
        "innisfree",
        "-i",
        &client_key_s,
        "-o",
        &known_hosts_opt,
        "-o",
        "ConnectTimeout=5",
        &ipv4_address,
    ];
    std::process::Command::new("ssh")
        .args(cmd_args)
        .status()
        .context("SSH interactive session failed")?;
    Ok(())
}

/// Spin up local network proxy to handle passing traffic
/// between the local service(s) and the remote server.
pub async fn run_proxy(
    local_ip: IpAddr,
    dest_ip: IpAddr,
    services: Vec<ServicePort>,
) -> Result<()> {
    // We'll kick off a dedicated proxy for each service,
    // and collect the handles to await them all together, concurrently.
    let mut tasks = vec![];
    for s in services {
        let listen_addr: SocketAddr = format!("{}:{}", local_ip, &s.local_port).parse()?;
        let dest_addr: SocketAddr = format!("{}:{}", dest_ip, &s.port).parse()?;
        let h = proxy_handler(listen_addr, dest_addr);
        tasks.push(h);
    }
    // We expect the proxies to block indefinitely, except ctrl+c.
    // If they return earlier, we'll be able to inspect the errors.
    let proxy_tasks = join_all(tasks).await;
    tracing::warn!("Proxy stopped unexpectedly, no longer forwarding traffic");
    for t in proxy_tasks {
        match t {
            Ok(t) => {
                // I don't expect to see this
                tracing::debug!("Service proxy returned ok: {:?}", t);
            }
            Err(e) => {
                return Err(anyhow!("Service proxy failed: {}", e));
            }
        }
    }
    Ok(())
}
