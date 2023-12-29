use anyhow::{anyhow, Context, Result};
use clap::{crate_version, Parser, Subcommand};
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
mod wg;

#[derive(Debug, Parser)]
#[clap(
    name = "innisfree",
    about = "Exposes local services on a public IPv4 address, via a cloud server.",
    version = crate_version!(),
)]
struct Args {
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
    // Set up logging via tracing-subscriber.
    let filter_layer = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new("info"))?;
    let fmt_layer = tracing_subscriber::fmt::layer()
        // .with_ansi(atty::is(atty::Stream::Stdout))
        .with_ansi(true)
        .with_target(true);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    let args = Args::parse();

    // Primary subcommand. Soup to nuts experience.
    match args.cmd {
        RootCommand::Up {
            name,
            ports,
            dest_ip,
            floating_ip,
        } => {
            // Ensure DigitalOcean API token is defined
            let _do_token = env::var("DIGITALOCEAN_API_TOKEN")
                .context("DIGITALOCEAN_API_TOKEN env var not set");
            let services = config::ServicePort::from_str_multi(&ports)?;
            tracing::info!("Will provide proxies for {:?}", services);
            let name = clean_name(&name);

            tracing::info!("Creating server '{}'", &name);
            let mgr: manager::TunnelManager =
                manager::TunnelManager::new(&name, services, floating_ip).await?;
            tracing::info!("Configuring server");
            match mgr.up() {
                Ok(_) => {
                    tracing::trace!("Up reports success");
                }
                Err(e) => {
                    tracing::error!("Failed bringing up tunnel: {}", e);
                    // Error probably unrecoverable
                    tracing::warn!("Attempting to exit gracefully...");
                    mgr.clean().await?;
                    std::process::exit(2);
                }
            }
            // Really need a better default case for floating-ip
            match floating_ip {
                Some(_f) => {
                    tracing::debug!("Configuring floating IP...");
                    unimplemented!("Floating IP support disabled due to trait refactor.");
                    // mgr.assign_floating_ip(f).await?;
                    // tracing::info!("Server ready! IPv4 address: {}", f);
                }
                None => {
                    let ip = &mgr.server.ipv4_address()?;
                    tracing::info!("Server ready! IPv4 address: {}", ip);
                }
            }
            if name == "innisfree" {
                tracing::debug!("Try logging in with 'innisfree ssh'");
            } else {
                tracing::debug!("Try logging in with 'innisfree ssh -n {}'", name);
            }
            let local_ip: IpAddr = mgr.wg.wg_local_device.interface.address;
            if &dest_ip.to_string() != "127.0.0.1" {
                tokio::spawn(manager::run_proxy(local_ip, dest_ip, mgr.services.clone()));
                mgr.block().await?;
            } else {
                tracing::info!(
                    "Ready to listen on {}. Start local services. Make sure to bind to {}, rather than 127.0.0.1!",
                    ports,
                    mgr.wg.wg_local_ip,
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
            manager::open_shell(&name).context(
                "Server not found. Try running 'innisfree up' first, or pass --name=<service>",
            )?;
        }

        RootCommand::Ip { name } => {
            let name = clean_name(&name);
            let ip = manager::get_server_ip(&name).context(
                "Server not found. Try running 'innisfree up' first, or pass --name=<service>.",
            )?;
            println!("{}", ip);
        }
        RootCommand::Doctor {} => {
            tracing::info!("Running doctor, to determine platform support...");
            doctor::platform_is_supported()?;
            tracing::info!("Platform support looks good! Ready to rock.");
        }
        RootCommand::Clean { name } => {
            tracing::info!("Cleaning config directory");
            let name = clean_name(&name);
            config::clean_config_dir(&name)?;
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
                .map_err(|e| anyhow!(format!("Proxy failed: {}", e)))?;
        }
    }
    Ok(())
}
