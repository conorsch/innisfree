// Logic to assign a pre-existing Floating IP resource in DigitalOcean
// to the Droplet used for managing the tunnel. Allows for DNS records
// to remain unchanged, but the tunnel to be rebuilt ad-hoc.
use anyhow::{Context, Result};
use serde_json::json;
use std::env;
use std::net::IpAddr;

const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/floating_ips";

pub struct FloatingIp {
    pub ip: IpAddr,
    pub droplet_id: u32,
}

impl FloatingIp {
    pub async fn assign(&self) -> Result<()> {
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let req_body = json!({
            "type": "assign",
            "droplet_id": self.droplet_id,
        });
        let request_url = DO_API_BASE_URL.to_owned() + "/" + &self.ip.to_string() + "/actions";

        let client = reqwest::Client::new();
        client
            .post(request_url)
            .json(&req_body)
            .bearer_auth(api_key)
            .send()
            .await
            .context("Network error, check connection")?;

        debug!("Assigning floating IP to droplet...");
        Ok(())
    }
}
