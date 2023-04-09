//! Logic to create a new, ephemeral SSH keypair in DigitalOcean.
//! Adding a new SSH key on instance creation prevents emails on
//! instance creation, providing a root pw.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use std::env;

const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2";

#[derive(Clone, Debug, Deserialize)]
/// Representation of an SSH public key, as defined by the DigitalOcean API.
/// For more information, see
/// <https://docs.digitalocean.com/reference/api/api-reference/#tag/SSH-Keys>.
pub struct DigitalOceanSshKey {
    /// Public key material, in ED25519 format, for the SSH keypair.
    pub public_key: String,
    /// Human-readable name for identifying the public key in the API console.
    pub name: String,
    /// Created automatically by DigitalOcean API, as a hash of the public key,
    /// to identify the public key uniquely.
    pub fingerprint: String,
    /// Numeric ID, created automatically by the DigitalOcean API, for this
    /// specific key. The ID can be used during Droplet creation requests
    /// to ensure a public key is present.
    pub id: u32,
}

/// Retrieves all SSH public keys assigned to the DigitalOcean account.
/// These keys will be inserted into the cloudinit file for the new host,
/// so that any user with access to the DigitalOcean account can log into
/// the innisfree host.
pub async fn get_all_keys() -> Result<Vec<DigitalOceanSshKey>> {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN")?;
    let request_url = DO_API_BASE_URL.to_owned() + "/account/keys";
    let client = reqwest::Client::new();
    debug!("Fetching SSH account public keys");
    let response = client
        .get(request_url)
        .bearer_auth(api_key)
        .send()
        .await?
        .error_for_status()?;
    let j: serde_json::Value = response.json().await?;
    let k: String = j["ssh_keys"].to_string();
    let ssh_keys: Vec<DigitalOceanSshKey> = serde_json::from_str(&k)?;
    Ok(ssh_keys)
}

impl DigitalOceanSshKey {
    /// Creates a new DigitalOceanSshKey based on the public key material passed in.
    /// A new key will be created via the API, so that a subsequent Droplet creation
    /// request can reference the DigitalOceanSshKey by its numeric ID.
    pub async fn new(name: String, public_key: String) -> Result<DigitalOceanSshKey> {
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let req_body = json!({
            "name": name,
            "public_key": public_key,
        });
        let request_url = DO_API_BASE_URL.to_owned() + "/account/keys";

        let client = reqwest::Client::new();
        let response = client
            .post(request_url)
            .json(&req_body)
            .bearer_auth(api_key)
            .send()
            .await?
            .error_for_status()?;

        debug!("Syncing SSH keypair to DigitalOcean...");
        let j: serde_json::Value = response.json().await?;
        let k: String = j["ssh_key"].to_string();
        let do_ssh_key: DigitalOceanSshKey = serde_json::from_str(&k)?;
        Ok(do_ssh_key)
    }
    /// Delete the DigitalOceanSshKey via the API.
    pub async fn destroy(&self) -> Result<()> {
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL.to_owned() + "/account/keys/" + &self.id.to_string();
        debug!("Deleting SSH keypair from DigitalOcean...");
        let client = reqwest::Client::new();
        let response = client
            .delete(request_url)
            .bearer_auth(api_key)
            .send()
            .await?
            .error_for_status();
        match response {
            Ok(_r) => Ok(()),
            Err(e) => Err(anyhow!("Failed to delete ssh key: {:?}", e)),
        }
    }
}
