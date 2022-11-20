//! Logic for managing a remote server via the DigitalOcean cloud provider.
//! Ideally the cloud provider logic would be generalized, but right now
//! DigitalOcean is the only supported provider.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use reqwest;
use serde;
use serde_json;
use std::env;
use std::net::IpAddr;
use std::thread;
use std::time;

use crate::server::digitalocean::ssh_key::DigitalOceanSshKey;

/// The zone in which the resources will be created, e.g. `sfo2`.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Regions>.
pub const DO_REGION: &str = "sfo2";
/// The type of VM instance to create, e.g. `s-1vcpu-1gb`.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Sizes>.
pub const DO_SIZE: &str = "s-1vcpu-1gb";
/// The OS choice for to base the Droplet on. Defaults to most recent Debian Stable.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Images>.
pub const DO_IMAGE: &str = "debian-11-x64";
const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/droplets";

/// Representation of a DigitalOcean Droplet, i.e. cloud VM.
/// See more documentation at
/// <https://docs.digitalocean.com/reference/api/api-reference/#tag/Droplets>.
#[derive(Debug, Deserialize)]
pub struct Droplet {
    /// Numeric ID, returned by API, to identify this Droplet.
    pub id: u32,
    // Human-readable name for Droplet, also its hostname.
    // name: String,
    /// Current state of server. Is `new` when booting, changes
    /// to `active` once host is booted and networking info is populated.
    pub status: String,
    /// Information about host networking, such as public and private
    /// interfaces and their corresponding IPv4/6 addresses. Use [Droplet::ipv4_address]
    /// to obtain an IP address easily.
    networks: HashMap<String, Vec<HashMap<String, String>>>,
    // The API takes a list, but we only care about 1 key,
    // the generated one, so use that.
    /// Optional dynamically generated SSH keypair, stored in cloud,
    /// used for initial connection (and to suppress emails about root
    /// passwords on instance creation).
    // TODO: Make this mandatory, since it's automatically created anyway.
    ssh_pubkey: Option<DigitalOceanSshKey>,
}

#[derive(Debug, Deserialize, Serialize)]
/// Template for building a request to create a new Droplet.
pub struct DropletConfig {
    /// The OS image used for creating the remote server. Defaults to [`DO_IMAGE`].
    pub image: String,
    /// Human-readable name for Droplet. Defaults to `innisfree`.
    name: String,
    /// The cloud region in which the server will be created. Defaults to [`DO_REGION`].
    region: String,
    /// The type of machine that will be created. Defaults to [`DO_SIZE`].
    /// See documentation for more options.
    size: String,
    /// Serialized content for a cloud-init YAML file.
    /// The [crate::manager::InnisfreeManager] will handle automatically generating
    /// cloud-init content with appropriate key material, via
    /// [crate::server::cloudinit::CloudConfig].
    /// See documentation for more information: <https://cloudinit.readthedocs.io/en/latest/>.
    user_data: String,
    /// List of SSH key IDs, as reported by the DigitalOcean API, for use
    /// during Droplet creation. Providing an SSH key ID during creation
    /// prevents emails from being sent to the account owner, providing
    /// a root password for the instance.
    ssh_keys: Vec<u32>,
}

impl DropletConfig {
    /// Creates a new [DropletConfig] based on the default implementation.
    pub fn new() -> Self {
        Default::default()
    }
}

impl Default for DropletConfig {
    fn default() -> Self {
        DropletConfig {
            image: DO_IMAGE.to_string(),
            name: "innisfree".to_string(),
            region: DO_REGION.to_string(),
            size: DO_SIZE.to_string(),
            user_data: String::default(),
            ssh_keys: vec![],
        }
    }
}

impl Droplet {
    /// Make an API request and create a new DigitalOcean droplet.
    /// Blocks until the server is "ready", which usually takes about 60 seconds.
    pub async fn new(name: &str, user_data: &str, public_key: String) -> Result<Droplet> {
        debug!("Creating new DigitalOcean Droplet");
        // Create new ephemeral ssh keypair
        let do_ssh_key = DigitalOceanSshKey::new(name.to_owned(), public_key).await?;
        let ssh_keys: Vec<u32> = vec![do_ssh_key.id];
        // Build JSON request body, for sending to DigitalOcean API
        let droplet_config = DropletConfig {
            name: name.to_string(),
            user_data: user_data.to_string(),
            ssh_keys,
            ..DropletConfig::new()
        };

        // The API logic could be abstracted further, in a DigitalOcean Manager.
        // Right now we only create Droplet resources, but an API Firewall would be nice.
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL;
        let client = reqwest::Client::new();

        let response = client
            .post(request_url)
            .json(&droplet_config)
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

    /// Block until a droplet is running. Upon creation, the API will
    /// return a result where `status="new"`. This method blocks until
    /// the API reports `state="running"`.
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

    /// Retrieves the public IPv4 address for the Droplet.
    /// Technically can fail, if results are missing from the API response.
    // TODO: IPv4 lookup can fail, should return Result to force handling.
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

/// Polls a droplet resource to get the latest data. Used during wait for boot,
/// to capture networking info like PublicIPv4, which is assigned after creation.
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
