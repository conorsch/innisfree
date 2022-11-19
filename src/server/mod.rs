// extern crate digitalocean;
// use digitalocean::prelude::*;
use std::collections::HashMap;
use std::env;
use std::net::IpAddr;
use std::thread;
use std::time;

use anyhow::{anyhow, Context, Result};

// Web API request imports, see
// https://rust-lang-nursery.github.io/rust-cookbook/web/clients/apis.html
extern crate reqwest;

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
use serde::Deserialize;
use serde_json::json;

mod cloudinit;
mod floating_ip;
mod ssh_key;
use self::cloudinit::generate_user_data;
use self::floating_ip::FloatingIp;
use self::ssh_key::DigitalOceanSshKey;
use crate::config::ServicePort;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;

const DO_REGION: &str = "sfo2";
const DO_SIZE: &str = "s-1vcpu-1gb";
const DO_IMAGE: &str = "debian-11-x64";
const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/droplets";

// Manager class, wraps a cloudserver VM type, such as Droplet,
// to make it a bit easier to work with. Bootstraps the necessary keypairs
// for services like SSH (both client and keyserver need keypairs), and Wireguard.
#[derive(Debug)]
pub struct InnisfreeServer {
    pub services: Vec<ServicePort>,
    pub ssh_client_keypair: SshKeypair,
    pub ssh_server_keypair: SshKeypair,
    // wg_mgr: WireguardManager,
    droplet: Droplet,
    // name: String,
}

impl InnisfreeServer {
    pub async fn new(
        name: &str,
        services: Vec<ServicePort>,
        wg_mgr: WireguardManager,
    ) -> Result<InnisfreeServer> {
        // Initialize variables outside struct, so we'll need to pass them around
        let ssh_client_keypair = SshKeypair::new("client")?;
        let ssh_server_keypair = SshKeypair::new("server")?;
        let user_data =
            generate_user_data(&ssh_client_keypair, &ssh_server_keypair, &wg_mgr, &services)
                .await?;
        let droplet = Droplet::new(name, &user_data, ssh_client_keypair.public.to_owned()).await?;
        Ok(InnisfreeServer {
            services,
            ssh_client_keypair,
            ssh_server_keypair,
            // wg_mgr,
            droplet,
            // name: name.to_string(),
        })
    }
    pub fn ipv4_address(&self) -> IpAddr {
        let droplet = &self.droplet;
        droplet.ipv4_address()
    }
    pub async fn assign_floating_ip(&self, floating_ip: &str) -> Result<()> {
        let fip: IpAddr = floating_ip.parse()?;
        let f = FloatingIp {
            ip: fip,
            droplet_id: self.droplet.id,
        };
        f.assign().await
    }
    pub async fn destroy(&self) -> Result<()> {
        // Destroys backing droplet
        self.droplet.destroy().await
    }
}

// Representation of a DigitalOcean Droplet, i.e. cloud VM.
#[derive(Debug, Deserialize)]
struct Droplet {
    id: u32,
    // name: String,
    status: String,
    networks: HashMap<String, Vec<HashMap<String, String>>>,
    // The API takes a list, but we only care about 1 key,
    // the generated one, so use that.
    ssh_pubkey: Option<DigitalOceanSshKey>,
}

impl Droplet {
    async fn new(name: &str, user_data: &str, public_key: String) -> Result<Droplet> {
        debug!("Creating new DigitalOcean Droplet");
        // Build JSON request body, for sending to DigitalOcean API
        let do_ssh_key = DigitalOceanSshKey::new(name.to_owned(), public_key).await?;
        let droplet_body = json!({
            "image": DO_IMAGE,
            "name": name,
            "region": DO_REGION,
            "size": DO_SIZE,
            "user_data": user_data,
            "ssh_keys": vec![do_ssh_key.id],
        });

        // The API logic could be abstracted further, in a DigitalOcean Manager.
        // Right now we only create Droplet resources, but an API Firewall would be nice.
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL;
        let client = reqwest::Client::new();

        let response = client
            .post(request_url)
            .json(&droplet_body)
            .bearer_auth(api_key)
            .send()
            .await?
            .error_for_status()?;

        let j: serde_json::Value = response.json().await?;
        let d: String = j["droplet"].to_string();
        let mut droplet: Droplet = serde_json::from_str(&d)?;
        // Add SSH key info after creation, since JSON response won't include it,
        // even though JSON request did. We'll need it to clean up in `self.destroy`.
        droplet.ssh_pubkey = Some(do_ssh_key);
        debug!("Server created, waiting for networking");
        droplet.wait_for_boot().await
    }

    async fn wait_for_boot(&self) -> Result<Droplet> {
        // The JSON response for droplet creation won't include info like
        // public IPv4 address, because that hasn't been assigned yet. The 'status'
        // field will show as "new", so wait until it's "active", then network info
        // will be populated. Might be a good use of enums here.
        loop {
            thread::sleep(time::Duration::from_secs(10));
            match get_droplet(self).await {
                Ok(droplet) => {
                    if droplet.status == "active" {
                        return Ok(droplet);
                    } else {
                        info!("Server still booting, waiting...");
                        continue;
                    }
                }
                Err(_) => {
                    return Err(anyhow!("Unknown error while waiting for droplet boot"));
                }
            }
        }
    }

    // IPv4 lookup can fail, should return Result to force handling.
    pub fn ipv4_address(&self) -> IpAddr {
        let mut s = String::new();
        for v4_network in &self.networks["v4"] {
            if v4_network["type"] == "public" {
                s = v4_network["ip_address"].clone();
                break;
            }
        }
        let ip: IpAddr = s.parse().unwrap();
        ip
    }

    /// Calls the API to destroy a droplet.
    pub async fn destroy(&self) -> Result<()> {
        if let Some(k) = &self.ssh_pubkey {
            k.destroy().await?;
        } else {
            warn!("No API pubkey associated with droplet, not destroying");
        }

        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL.to_owned() + "/" + &self.id.to_string();

        let client = reqwest::Client::new();
        client
            .delete(request_url)
            .bearer_auth(api_key)
            .send()
            .await?
            .error_for_status()
            .context("Failed to destroy droplet")?;

        debug!("Droplet destroyed");
        Ok(())
    }
}

// Polls a droplet resource to get the latest data. Used during wait for boot,
// to capture networking info like PublicIPv4, which is assigned after creation.
async fn get_droplet(droplet: &Droplet) -> Result<Droplet> {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL.to_owned() + "/" + &droplet.id.to_string();

    let client = reqwest::Client::new();
    let response = client
        .get(request_url)
        .bearer_auth(api_key)
        .send()
        .await?
        .error_for_status()?;
    let j: serde_json::Value = response.json().await?;
    let d_s: String = j["droplet"].to_string();
    let mut d: Droplet = serde_json::from_str(&d_s)?;
    d.ssh_pubkey = droplet.ssh_pubkey.clone();
    Ok(d)
}
