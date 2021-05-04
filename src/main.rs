use clap::Arg;
use clap::{crate_version, App};
use std::env;
use std::error::Error;
use std::sync::Arc;

#[macro_use]
extern crate log;
use env_logger::Env;

extern crate ctrlc;

// Innisfree imports
mod cloudinit;
mod config;
mod floating_ip;
mod manager;
mod proxy;
mod server;
mod ssh;
mod wg;
use crate::wg::WIREGUARD_LOCAL_IP;

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
                    Arg::new("ports")
                        .about("list of service ports to forward, comma-separated")
                        .default_value("8080/TCP,443/TCP")
                        .long("ports")
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("IPv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
                        .long("dest-ip")
                        .short('d'),
                )
                .arg(
                    Arg::new("floating-ip")
                        .about("Declare pre-existing Floating IP to attach to Droplet")
                        // Figure out how to default to an empty string
                        .default_value("None")
                        .long("floating-ip")
                        .short('f'),
                ),
        )
        .subcommand(App::new("ssh").about("Open interactive SSH shell on cloud node"))
        .subcommand(App::new("ip").about("Display IPv4 address for cloud node"))
        .subcommand(
            App::new("proxy")
                .about("Start process to forward traffic, assumes tunnel already up")
                .arg(
                    Arg::new("ports")
                        .about("list of service ports to forward, comma-separated")
                        .default_value("8080/TCP,443/TCP")
                        .long("ports")
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("IPv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
                        .long("dest-ip")
                        .short('d'),
                ),
        )
        .get_matches();

    // Primary subcommand. Soup to nuts experience.
    if let Some(ref matches) = matches.subcommand_matches("up") {
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

        let dest_ip = matches.value_of("dest-ip").unwrap().to_owned();
        let port_spec = matches.value_of("ports").unwrap();
        let floating_ip = matches.value_of("floating-ip").unwrap();
        let services = config::ServicePort::from_str_multi(port_spec);
        info!("Will provide proxies for {:?}", services);

        info!("Creating server");
        let mgr = match manager::InnisfreeManager::new(services) {
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
                let _ = mgr.clean();
                std::process::exit(2);
            }
        }

        let mgr_ctrlc = mgr.clone();
        ctrlc::set_handler(move || {
            warn!("Caught ctrl+c, exiting gracefully");
            mgr_ctrlc.clean();
            debug!("Clean up complete, exiting!");
            std::process::exit(0);
        })
        .expect("Error setting Ctrl-C handler");

        // Really need a better default case for floating-ip
        if floating_ip != "None" {
            debug!("Configuring floating IP...");
            mgr.assign_floating_ip(floating_ip);
            info!("Server ready! IPv4 address: {}", floating_ip);
        } else {
            let ip = &mgr.server.ipv4_address();
            info!("Server ready! IPv4 address: {}", ip);
        }
        debug!("Try logging in with 'innisfree ssh'");
        let local_ip = String::from(WIREGUARD_LOCAL_IP);
        if dest_ip != "127.0.0.1" {
            manager::run_proxy(local_ip, dest_ip, mgr.services.clone()).await;
        } else {
            info!(
                "Ready to listen on {}. Start local services. Make sure to bind to 0.0.0.0, rather than 127.0.0.1!",
                port_spec
            );
            debug!("Blocking forever. Press ctrl+c to tear down the tunnel and destroy server.");
            // Block forever, ctrl+c will interrupt
            loop {
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        }
    } else if let Some(ref _matches) = matches.subcommand_matches("ssh") {
        let result = manager::open_shell();
        match result {
            Ok(_) => trace!("Interactive SSH session completed successfully"),
            Err(_) => {
                error!("Server not found. Try running 'innisfree up' first");
                std::process::exit(3);
            }
        }
    } else if let Some(ref _matches) = matches.subcommand_matches("ip") {
        let ip = manager::get_server_ip();
        match ip {
            Ok(ip) => {
                println!("{}", ip);
            }
            Err(_) => {
                error!("Server not found. Try running 'innisfree up' first.");
                std::process::exit(2);
            }
        }
    } else if let Some(ref matches) = matches.subcommand_matches("proxy") {
        warn!("Subcommand 'proxy' only intended for debugging, it assumes tunnel exists already");
        let dest_ip = matches.value_of("dest-ip").unwrap().to_owned();
        let port_spec = matches.value_of("ports").unwrap();
        let ports = config::ServicePort::from_str_multi(port_spec);
        let local_ip = String::from(WIREGUARD_LOCAL_IP);
        info!("Starting proxy for services {:?}", ports);
        manager::run_proxy(local_ip, dest_ip, ports).await;
    }

    Ok(())
}
