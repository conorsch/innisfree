// extern crate digitalocean;
// use digitalocean::prelude::*;
use std::collections::HashMap;
use std::env;
use std::thread;
use std::time;

// Web API request imports, see
// https://rust-lang-nursery.github.io/rust-cookbook/web/clients/apis.html
extern crate reqwest;

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
use serde::Deserialize;
use serde_json::json;

use crate::cloudinit::generate_user_data;
use crate::config::{make_config_dir, InnisfreeError, ServicePort};
use crate::floating_ip::FloatingIp;
use crate::ssh::SshKeypair;
use crate::wg::WireguardDevice;

const DO_REGION: &str = "sfo2";
const DO_SIZE: &str = "s-1vcpu-1gb";
const DO_IMAGE: &str = "ubuntu-20-04-x64";
const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/droplets";

// Manager class, wraps a cloudserver VM type, such as Droplet,
// to make it a bit easier to work with. Bootstraps the necessary keypairs
// for services like SSH (both client and keyserver need keypairs), and Wireguard.
#[derive(Debug)]
pub struct InnisfreeServer {
    pub services: Vec<ServicePort>,
    pub ssh_client_keypair: SshKeypair,
    pub ssh_server_keypair: SshKeypair,
    wg_device: WireguardDevice,
    droplet: Droplet,
    name: String,
}

impl InnisfreeServer {
    pub fn new(
        name: &str,
        services: Vec<ServicePort>,
        wg_device: WireguardDevice,
    ) -> Result<InnisfreeServer, InnisfreeError> {
        // Initialize variables outside struct, so we'll need to pass them around
        let ssh_client_keypair = SshKeypair::new("client")?;
        let ssh_server_keypair = SshKeypair::new("server")?;
        let user_data = generate_user_data(
            &ssh_client_keypair,
            &ssh_server_keypair,
            &wg_device,
            &services,
        )?;
        let droplet = Droplet::new(&name, &user_data)?;
        Ok(InnisfreeServer {
            services,
            ssh_client_keypair,
            ssh_server_keypair,
            wg_device,
            droplet,
            name: name.to_string(),
        })
    }
    pub fn ipv4_address(&self) -> String {
        let droplet = &self.droplet;
        droplet.ipv4_address()
    }
    pub fn assign_floating_ip(&self, floating_ip: &str) {
        let f = FloatingIp {
            ip: floating_ip.to_owned(),
            droplet_id: self.droplet.id,
        };
        f.assign();
    }
    // Dead code because it's debug-only, might want again.
    #[allow(dead_code)]
    pub fn write_user_data(&self) {
        // Write full config locally for debugging;
        let user_data = generate_user_data(
            &self.ssh_client_keypair,
            &self.ssh_server_keypair,
            &self.wg_device,
            &self.services,
        )
        .unwrap();
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("cloudinit.cfg");
        std::fs::write(&fpath.to_str().unwrap(), &user_data).expect("Failed to create cloud-init");
    }
    pub fn destroy(&self) {
        // Destroys backing droplet
        self.droplet.destroy();
    }
}

// Representation of a DigitalOcean Droplet, i.e. cloud VM.
#[derive(Debug, Deserialize)]
struct Droplet {
    id: u32,
    name: String,
    status: String,
    networks: HashMap<String, Vec<HashMap<String, String>>>,
}

impl Droplet {
    fn new(name: &str, user_data: &str) -> Result<Droplet, InnisfreeError> {
        debug!("Creating new DigitalOcean Droplet");
        // Build JSON request body, for sending to DigitalOcean API
        let droplet_body = json!({
            "image": DO_IMAGE,
            "name": name,
            "region": DO_REGION,
            "size": DO_SIZE,
            "user_data": user_data,
        });

        // The API logic could be abstracted further, in a DigitalOcean Manager.
        // Right now we only create Droplet resources, but an API Firewall would be nice.
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL;
        let client = reqwest::blocking::Client::new();

        let response = client
            .post(request_url)
            .json(&droplet_body)
            .bearer_auth(api_key)
            .send()?;

        let j: serde_json::Value = response.json().unwrap();
        let d: String = j["droplet"].to_string();
        let droplet: Droplet = serde_json::from_str(&d).unwrap();
        debug!("Server created, waiting for networking");
        Ok(droplet.wait_for_boot())
    }

    fn wait_for_boot(&self) -> Droplet {
        // The JSON response for droplet creation won't include info like
        // public IPv4 address, because that hasn't been assigned yet. The 'status'
        // field will show as "new", so wait until it's "active", then network info
        // will be populated. Might be a good use of enums here.
        loop {
            thread::sleep(time::Duration::from_secs(10));
            let droplet: Droplet = get_droplet(&self);
            if droplet.status == "active" {
                return droplet;
            } else {
                info!("Server still booting, waiting...");
            }
        }
    }

    // IPv4 lookup can fail, should return Result to force handling.
    pub fn ipv4_address(&self) -> String {
        let mut ip: String = "".to_string();
        for v4_network in &self.networks["v4"] {
            if v4_network["type"] == "public" {
                ip = v4_network["ip_address"].clone();
                break;
            }
        }
        ip
    }
    pub fn destroy(&self) {
        destroy_droplet(&self);
    }
}

// Polls a droplet resource to get the latest data. Used during wait for boot,
// to capture networking info like PublicIPv4, which is assigned after creation.
fn get_droplet(droplet: &Droplet) -> Droplet {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL.to_owned() + "/" + &droplet.id.to_string();

    let client = reqwest::blocking::Client::new();
    let response = client.get(request_url).bearer_auth(api_key).send().unwrap();

    let j: serde_json::Value = response.json().unwrap();
    let d: String = j["droplet"].to_string();
    serde_json::from_str(&d).unwrap()
}

// Calls the API to destroy a droplet.
fn destroy_droplet(droplet: &Droplet) {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL.to_owned() + "/" + &droplet.id.to_string();

    let client = reqwest::blocking::Client::new();
    let response = client.delete(request_url).bearer_auth(api_key).send();
    match response {
        Ok(_) => {
            debug!("Destroying droplet");
        }
        Err(e) => {
            error!("Failed to destroy droplet: {}", e);
        }
    }
}
