use clap::Arg;
use clap::{crate_version, App};
use std::env;
use std::error::Error;
use std::net::IpAddr;
use std::sync::Arc;

#[macro_use]
extern crate log;
use env_logger::Env;

// Innisfree imports
mod config;
mod doctor;
mod error;
mod manager;
mod net;
mod proxy;
mod server;
mod ssh;
mod wg;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Activate env_logger https://github.com/env-logger-rs/env_logger
    // The `Env` lets us tweak what the environment
    // variables to read are and what the default
    // value is if they're missing
    let env = Env::default().filter_or("RUST_LOG", "debug,reqwest=info");
    env_logger::init_from_env(env);
    let matches = App::new("Innisfree")
        .version(crate_version!())
        .about("Exposes local services on a public IPv4 address, via a cloud server.")
        .subcommand(
            App::new("up")
                .about("Create new innisfree tunnel")
                .arg(
                    Arg::new("name")
                        .about("title for the service, used for cloud node and systemd service")
                        .default_value("innisfree")
                        .env("INNISFREE_NAME")
                        .long("name")
                        .short('n'),
                )
                .arg(
                    Arg::new("ports")
                        .about("list of service ports to forward, comma-separated")
                        .default_value("8080/TCP,443/TCP")
                        .env("INNISFREE_PORTS")
                        .long("ports")
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("IPv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
                        .env("INNISFREE_DEST_IP")
                        .long("dest-ip")
                        .short('d'),
                )
                .arg(
                    Arg::new("floating-ip")
                        .about("Declare pre-existing Floating IP to attach to Droplet")
                        // Figure out how to default to an empty string
                        .default_value("None")
                        .env("INNISFREE_FLOATING_IP")
                        .long("floating-ip")
                        .short('f'),
                ),
        )
        .subcommand(
            App::new("ssh")
                .about("Open interactive SSH shell on cloud node")
                .arg(
                    Arg::new("name")
                        .about("title for the service, used for cloud node and systemd service")
                        .default_value("innisfree")
                        .env("INNISFREE_NAME")
                        .long("name")
                        .short('n'),
                ),
        )
        .subcommand(
            App::new("ip")
                .about("Display IPv4 address for cloud node")
                .arg(
                    Arg::new("name")
                        .about("title for the service, used for cloud node and systemd service")
                        .default_value("innisfree")
                        .env("INNISFREE_NAME")
                        .long("name")
                        .short('n'),
                ),
        )
        .subcommand(App::new("doctor").about("Run checks to evaluate platform support"))
        .subcommand(
            App::new("proxy")
                .about("Start process to forward traffic, assumes tunnel already up")
                .arg(
                    Arg::new("ports")
                        .about("list of service ports to forward, comma-separated")
                        .default_value("8080/TCP,443/TCP")
                        .env("INNISFREE_PORTS")
                        .long("ports")
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("IPv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
                        .env("INNISFREE_DEST_IP")
                        .long("dest-ip")
                        .short('d'),
                ),
        )
        .get_matches();

    // Primary subcommand. Soup to nuts experience.
    if let Some(matches) = matches.subcommand_matches("up") {
        // Ensure DigitalOcean API token is defined
        let do_token;
        match env::var("DIGITALOCEAN_API_TOKEN") {
            Ok(val) => do_token = val,
            Err(_e) => do_token = "".to_string(),
        }
        if do_token.is_empty() {
            error!("DIGITALOCEAN_API_TOKEN env var not set");
            std::process::exit(1);
        }

        let dest_ip: IpAddr = matches.value_of("dest-ip").unwrap().parse().unwrap();
        let port_spec = matches.value_of("ports").unwrap();
        let floating_ip = matches.value_of("floating-ip").unwrap();
        let tunnel_name = config::clean_name(matches.value_of("name").unwrap());
        let services = config::ServicePort::from_str_multi(port_spec);
        info!("Will provide proxies for {:?}", services);

        info!("Creating server '{}'", &tunnel_name);
        let mgr = match manager::InnisfreeManager::new(&tunnel_name, services).await {
            Ok(m) => m,
            Err(e) => {
                error!("{}", e);
                std::process::exit(2);
            }
        };
        let mgr = Arc::new(mgr);
        info!("Configuring server");
        match mgr.up() {
            Ok(_) => {
                trace!("Up reports success");
            }
            Err(e) => {
                error!("Failed bringing up tunnel: {}", e);
                // Error probably unrecoverable
                warn!("Attempting to exit gracefully...");
                mgr.clean().await;
                std::process::exit(2);
            }
        }

        // Really need a better default case for floating-ip
        if floating_ip != "None" {
            debug!("Configuring floating IP...");
            mgr.assign_floating_ip(floating_ip).await?;
            info!("Server ready! IPv4 address: {}", floating_ip);
        } else {
            let ip = &mgr.server.ipv4_address();
            info!("Server ready! IPv4 address: {}", ip);
        }
        if tunnel_name == "innisfree" {
            debug!("Try logging in with 'innisfree ssh'");
        } else {
            debug!("Try logging in with 'innisfree ssh -n {}'", tunnel_name);
        }
        let local_ip: IpAddr = mgr.wg.wg_local_device.interface.address;
        if &dest_ip.to_string() != "127.0.0.1" {
            tokio::spawn(manager::run_proxy(local_ip, dest_ip, mgr.services.clone()));
            mgr.block().await;
        } else {
            info!(
                "Ready to listen on {}. Start local services. Make sure to bind to {}, rather than 127.0.0.1!",
                port_spec,
                mgr.wg.wg_local_ip,
            );
            debug!("Blocking forever. Press ctrl+c to tear down the tunnel and destroy server.");
            // Block forever, ctrl+c will interrupt
            mgr.block().await;
        }
    } else if let Some(matches) = matches.subcommand_matches("ssh") {
        let tunnel_name = config::clean_name(matches.value_of("name").unwrap());
        let result = manager::open_shell(&tunnel_name);
        match result {
            Ok(_) => trace!("Interactive SSH session completed successfully"),
            Err(_) => {
                error!(
                    "Server not found. Try running 'innisfree up' first, or pass --name=<service>"
                );
                std::process::exit(3);
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("ip") {
        let tunnel_name = config::clean_name(matches.value_of("name").unwrap());
        let ip = manager::get_server_ip(&tunnel_name);
        match ip {
            Ok(ip) => {
                println!("{}", ip);
            }
            Err(_) => {
                error!(
                    "Server not found. Try running 'innisfree up' first, or pass --name=<service>."
                );
                std::process::exit(2);
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("proxy") {
        warn!("Subcommand 'proxy' only intended for debugging, it assumes tunnel exists already");
        let dest_ip: IpAddr = matches.value_of("dest-ip").unwrap().parse().unwrap();
        let port_spec = matches.value_of("ports").unwrap();
        let ports = config::ServicePort::from_str_multi(port_spec);
        let local_ip: IpAddr = "127.0.0.1".parse().unwrap();
        warn!("Ctrl+c will not halt proxy, use ctrl+z and `kill -9 %1`");
        info!("Starting proxy for services {:?}", ports);
        match manager::run_proxy(local_ip, dest_ip, ports).await {
            Ok(_) => {}
            Err(e) => {
                error!("Proxy failed: {}", e);
                std::process::exit(4);
            }
        };
    } else if let Some(_matches) = matches.subcommand_matches("doctor") {
        info!("Running doctor, to determine platform support...");
        match doctor::platform_is_supported() {
            Ok(result) => {
                if result {
                    info!("Platform support looks good! Ready to rock.");
                } else {
                    error!("Found some problems. Resolve those, then rerun doctor.");
                }
            }
            Err(_) => {
                error!(
                    "Platform is not supported. =( \
                       Only recent Linux distros such as Debian, Ubuntu, or Fedora are supported."
                );
                std::process::exit(5);
            }
        }
    }

    Ok(())
}
