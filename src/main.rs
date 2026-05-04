use anyhow::{anyhow, Context, Result};
use clap::{crate_version, ArgAction, Parser, Subcommand};
use config::clean_name;
use std::env;
use std::net::IpAddr;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::{prelude::*, EnvFilter};

// Innisfree imports
mod config;
mod doctor;
mod manager;
mod net;
mod proxy;
mod server;
mod ssh;
mod state;
mod wg;

use crate::server::digitalocean::client::DoClient;
use crate::server::digitalocean::provider::DigitalOceanProvider;
use crate::server::Provider;

#[derive(Debug, Parser)]
#[clap(
    name = "innisfree",
    about = "Exposes local services on a public IPv4 address, via a cloud server.",
    version = crate_version!(),
)]
struct Args {
    /// Increase log verbosity. `-v` enables debug, `-vv` enables trace.
    /// Ignored if `RUST_LOG` is set in the environment.
    #[clap(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Create new innisfree tunnel
    #[clap(subcommand)]
    cmd: RootCommand,
}

#[derive(Debug, Subcommand)]
enum RootCommand {
    /// Exposes local services on a public IPv4 address, via a cloud server
    Up {
        /// Title for the service, used for cloud node and systemd service
        #[clap(default_value = "innisfree", long, short, env = "INNISFREE_NAME")]
        name: String,

        /// List of service ports to forward, comma-separated. Specified as:
        /// `<PORT>[:<LOCAL_PORT>][/PROTOCOL]. For example, the default value `80:8000/TCP`
        /// will publish `80/TCP` on the external ingress, forwarding traffic
        /// to `8000/TCP` on the dest ip.
        #[clap(default_value = "80:8000/TCP", env = "INNISFREE_PORTS", long, short)]
        ports: String,

        /// IPv4 Address of proxy destination, whither traffic is forwarded
        #[clap(default_value = "127.0.0.1", env = "INNISFREE_DEST_IP", long, short)]
        dest_ip: IpAddr,

        /// Declare pre-existing Floating IP to attach to Droplet"
        #[clap(env = "INNISFREE_FLOATING_IP", long, short)]
        floating_ip: Option<IpAddr>,
    },

    /// Open interactive SSH shell on cloud node
    Ssh {
        /// Title for the service, used for cloud node and systemd service
        #[clap(default_value = "innisfree", env = "INNISFREE_NAME", long, short)]
        name: String,
    },

    /// Display IPv4 address for cloud node
    Ip {
        /// Title for the service, used for cloud node and systemd service
        #[clap(default_value = "innisfree", env = "INNISFREE_NAME", long, short)]
        name: String,

        /// Emit `{"ip": "..."}` instead of a bare address, for piping
        /// into other CLI tools.
        #[clap(long)]
        json: bool,
    },

    /// Run checks to evaluate platform support
    Doctor {},

    /// Clean local config directory.
    Clean {
        /// Title for the service, used for cloud node and systemd service
        #[clap(default_value = "innisfree", long, short, env = "INNISFREE_NAME")]
        name: String,
    },

    /// Start process to forward traffic, assumes tunnel already up
    Proxy {
        /// List of service ports to forward, comma-separated.
        /// Each pair of service ports should be colon-separated
        /// between local and remote ports: e.g. "8000:80" means
        /// that a local service on 8000/TCP will receive traffic
        /// sent to 80/TCP on the remote cloud node.
        #[clap(default_value = "8000:80", env = "INNISFREE_PORTS", long, short)]
        ports: String,

        /// IPv4 Address of proxy destination, whither traffic is forwarded.
        #[clap(default_value = "127.0.0.1", env = "INNISFREE_DEST_IP", long, short)]
        dest_ip: IpAddr,
    },
}

#[tokio::main]
/// Runs the `innisfree` CLI. Pass arguments to configure
/// local services that should be exposed remotely.
/// Pass `--help` for information.
async fn main() -> Result<()> {
    let args = Args::parse();

    // Set up logging via tracing-subscriber. `RUST_LOG` always wins so
    // operators can override per-module filters; otherwise `-v` / `-vv`
    // pick the level. All output goes to stderr so subcommands like
    // `innisfree ip` can keep stdout clean for piping.
    let filter_layer = match (env::var("RUST_LOG"), args.verbose) {
        (Ok(_), _) => EnvFilter::from_default_env(),
        (Err(_), 0) => EnvFilter::new("info"),
        (Err(_), 1) => EnvFilter::new("debug"),
        (Err(_), _) => EnvFilter::new("trace"),
    };
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    // Primary subcommand. Soup to nuts experience.
    match args.cmd {
        RootCommand::Up {
            name,
            ports,
            dest_ip,
            floating_ip,
        } => {
            // Construct the DO client up front (reads DIGITALOCEAN_API_TOKEN
            // exactly once) and wrap it in the provider trait object so the
            // rest of the code path stays backend-agnostic.
            let provider: Box<dyn Provider> =
                Box::new(DigitalOceanProvider::new(DoClient::from_env()?));
            let services = config::ServicePort::from_str_multi(&ports)?;
            tracing::info!("Will provide proxies for {:?}", services);
            let name = clean_name(&name);

            tracing::info!("Creating server '{}'", &name);
            let mut mgr: manager::TunnelManager =
                manager::TunnelManager::new(provider, &name, services, floating_ip).await?;
            tracing::info!("Configuring server");
            match mgr.up().await {
                Ok(_) => {
                    tracing::trace!("Up reports success");
                }
                Err(e) => {
                    // `{:#}` includes the anyhow context chain on one line;
                    // plain `{}` only prints the outer wrap and hides the
                    // underlying cause (was masking real LocalWg::start
                    // errors during integration tests).
                    tracing::error!("Failed bringing up tunnel: {:#}", e);
                    // Error probably unrecoverable. Best-effort cleanup
                    // — log a clean-up failure loudly but never let it
                    // mask the original tunnel error.
                    tracing::warn!("Attempting to exit gracefully...");
                    if let Err(clean_err) = mgr.clean().await {
                        tracing::error!(
                            "cleanup also failed (manual cleanup may be required): {clean_err:#}"
                        );
                    }
                    std::process::exit(2);
                }
            }
            // The floating-IP attachment itself happens inside
            // TunnelManager::new (via InnisfreeServer::assign_floating_ip);
            // here we just report whichever address users will actually hit.
            let ready_ip = match floating_ip {
                Some(f) => f,
                None => mgr.server_ipv4()?,
            };
            tracing::info!("Server ready! IPv4 address: {}", ready_ip);
            if name == "innisfree" {
                tracing::debug!("Try logging in with 'innisfree ssh'");
            } else {
                tracing::debug!("Try logging in with 'innisfree ssh -n {}'", name);
            }
            let local_ip = mgr.local_wg_address();
            if &dest_ip.to_string() != "127.0.0.1" {
                tokio::spawn(manager::run_proxy(
                    local_ip,
                    dest_ip,
                    mgr.services().to_vec(),
                ));
                mgr.block().await?;
            } else {
                tracing::info!(
                    "Ready to listen on {}. Start local services. Make sure to bind to {}, rather than 127.0.0.1!",
                    ports,
                    local_ip,
                );
                tracing::debug!(
                    "Blocking forever. Press ctrl+c to tear down the tunnel and destroy server."
                );
                // Block forever, ctrl+c will interrupt
                mgr.block().await?;
            }
        }
        RootCommand::Ssh { name } => {
            let name = clean_name(&name);
            manager::open_shell(&name).await.context(
                "Server not found. Try running 'innisfree up' first, or pass --name=<service>",
            )?;
        }

        RootCommand::Ip { name, json } => {
            let name = clean_name(&name);
            let ip = manager::get_server_ip(&name).context(
                "Server not found. Try running 'innisfree up' first, or pass --name=<service>.",
            )?;
            if json {
                println!("{}", serde_json::json!({ "ip": ip.to_string() }));
            } else {
                println!("{}", ip);
            }
        }
        RootCommand::Doctor {} => {
            tracing::info!("Running doctor, to determine platform support...");
            doctor::platform_is_supported()?;
            tracing::info!("Platform support looks good! Ready to rock.");
        }
        RootCommand::Clean { name } => {
            tracing::info!("Cleaning state directory");
            let name = clean_name(&name);
            state::remove_state_for_service(&name)?;
        }

        RootCommand::Proxy { ports, dest_ip } => {
            tracing::warn!(
                "Subcommand 'proxy' only intended for debugging, it assumes tunnel exists already"
            );
            tracing::debug!(
                "Blocking forever. Press ctrl+c to tear down the tunnel and destroy server."
            );

            // Block forever, ctrl+c will interrupt
            let ports = config::ServicePort::from_str_multi(&ports)?;
            let local_ip: IpAddr = "127.0.0.1".parse()?;
            tracing::warn!("Ctrl+c will not halt proxy, use ctrl+z and `kill -9 %1`");
            tracing::info!("Starting proxy for services {:?}", ports);
            manager::run_proxy(local_ip, dest_ip, ports)
                .await
                .map_err(|e| anyhow!("Proxy failed: {}", e))?;
        }
    }
    Ok(())
}
