use serde::Deserialize;
/// Logic to create a new, ephemeral SSH keypair in DigitalOcean.
/// Adding a new SSH key on instance creation prevents emails on
/// instance creation, providing a root pw.
use serde_json::json;
use std::env;

use crate::error::InnisfreeError;

const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2";

#[derive(Clone, Debug, Deserialize)]
pub struct DigitalOceanSshKey {
    pub public_key: String,
    pub name: String,
    pub fingerprint: String,
    pub id: u32,
}

pub async fn get_all_keys() -> Result<Vec<DigitalOceanSshKey>, InnisfreeError> {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
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
    pub async fn new(
        name: String,
        public_key: String,
    ) -> Result<DigitalOceanSshKey, InnisfreeError> {
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
    pub async fn destroy(&self) -> Result<(), InnisfreeError> {
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
            Err(e) => {
                error!("Failed to delete ssh key: {:?}", e);
                Err(InnisfreeError::Unknown)
            }
        }
    }
}
