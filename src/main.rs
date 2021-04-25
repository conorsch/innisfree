use clap::App;
use clap::Arg;
use std::env;

// Innisfree imports
mod config;
mod manager;
mod server;
mod ssh;
mod wg;
// use server;

#[macro_use]
extern crate log;

use env_logger::Env;

fn main() {
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
                        .default_value("80/TCP,443/TCP")
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
        .get_matches();

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

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level app
    if let Some(ref matches) = matches.subcommand_matches("up") {
        warn!("Subcommand 'up' is only partially implemented");
        if !matches.is_present("dest-ip") {
            warn!("Yo bro, the dest-ip is required...");
        }
        let port_spec = matches.value_of("ports").unwrap();
        let p = config::ServicePort::from_str_multi(port_spec);
        info!("ServicePorts: {:?}", p);

        info!("Creating server");
        let mgr = manager::InnisfreeManager::new(p);
        info!("Configuring server");
        mgr.up();

        let ip = &mgr.server.ipv4_address();
        info!("Server IPv4 address: {:?}", ip);
        debug!("Try logging in with 'innisfree ssh'");
    }

    if let Some(ref _matches) = matches.subcommand_matches("ssh") {
        warn!("Subcommand 'ssh' is only partially implemented; it assumes server exists");
        let ip = manager::get_server_ip().unwrap();
        info!("Found server IPv4 address: {:?}", ip);
        debug!("Attempting to open interactive shell");
        manager::open_shell();
    }

    if let Some(ref _matches) = matches.subcommand_matches("ip") {
        warn!("Subcommand 'ip' is only partially implemented; it assumes server exists");
        let ip = manager::get_server_ip().unwrap();
        debug!("Found ip address: {:?}", ip);
        println!("{}", ip);
    }

    // Continued program logic goes here...
}
