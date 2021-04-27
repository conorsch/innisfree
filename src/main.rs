use clap::App;
use clap::Arg;
use std::env;
use std::error::Error;

#[macro_use]
extern crate log;
use env_logger::Env;

// Innisfree imports
mod config;
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
    let env = Env::default().filter_or("RUST_LOG", "debug");
    env_logger::init_from_env(env);
    let matches = App::new("Innisfree")
        .version("0.1.1")
        .author("Conor Schaefer <conor@ruin.dev")
        .about("Exposes local services on a public IPv4 address, via DigitalOcean")
        .subcommand(
            App::new("up")
                .about("Create new innisfree tunnel")
                .arg(
                    Arg::new("ports")
                        .about("list of service ports to forward, comma-separated")
                        .default_value("8080/TCP,443/TCP")
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("Ipv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
                        .short('d'),
                )
                .arg(
                    Arg::new("floating-ip")
                        .about("Declare pre-existing Floating IP to attach to Droplet")
                        // Figure out how to default to an empty string
                        .default_value("None")
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
                        .short('p'),
                )
                .arg(
                    Arg::new("dest-ip")
                        .about("Ipv4 Address of proxy destination, whither traffic is forwarded")
                        .default_value("127.0.0.1")
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

        warn!("Subcommand 'up' is only partially implemented; you must run 'proxy' separately");
        let dest_ip = matches.value_of("dest-ip").unwrap().to_owned();
        let port_spec = matches.value_of("ports").unwrap();
        let services = config::ServicePort::from_str_multi(port_spec);
        info!("Will provide proxies for {:?}", services);

        info!("Creating server");
        let mgr = manager::InnisfreeManager::new(services);
        info!("Configuring server");
        mgr.up();

        let ip = &mgr.server.ipv4_address();
        info!("Server IPv4 address: {:?}", ip);
        debug!("Try logging in with 'innisfree ssh'");
        debug!("Etnering proxy jawns");
        let local_ip = String::from(WIREGUARD_LOCAL_IP);
        manager::run_proxy(local_ip, dest_ip, mgr.services).await;
    } else if let Some(ref _matches) = matches.subcommand_matches("ssh") {
        warn!("Subcommand 'ssh' is only partially implemented; it assumes server exists");
        let ip = manager::get_server_ip().unwrap();
        info!("Found server IPv4 address: {:?}", ip);
        debug!("Attempting to open interactive shell");
        manager::open_shell();
    } else if let Some(ref _matches) = matches.subcommand_matches("ip") {
        warn!("Subcommand 'ip' is only partially implemented; it assumes server exists");
        let ip = manager::get_server_ip().unwrap();
        println!("{}", ip);
    } else if let Some(ref matches) = matches.subcommand_matches("proxy") {
        warn!("Subcommand 'proxy' only intended for debugging, it assumes tunnel exists already");
        let dest_ip = matches.value_of("dest-ip").unwrap().to_owned();
        let port_spec = matches.value_of("ports").unwrap();
        let ports = config::ServicePort::from_str_multi(port_spec);
        let local_ip = String::from(WIREGUARD_LOCAL_IP);

        manager::run_proxy(local_ip, dest_ip, ports).await;
    }

    Ok(())
}
