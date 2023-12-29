//! Logic to assign a pre-existing Floating IP resource in DigitalOcean
//! to the Droplet used for managing the tunnel. Allows for DNS records
//! to remain unchanged, but the tunnel to be rebuilt ad-hoc.
use anyhow::{Context, Result};
use serde_json::json;
use std::env;
use std::net::IpAddr;

const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/floating_ips";

/// Represents a DigitalOcean Reserved IP (FKA Floating IP).
/// In order to use a Floating IP with DigitalOcean, first create it out of band,
/// via the DigitalOcean web console. Once the Floating IP exists on your account,
/// you can then pass it in on the CLI with `--floating-ip`.
/// See documentation for more information: <https://docs.digitalocean.com/products/networking/reserved-ips/>.
pub struct FloatingIp {
    /// The Floating IP address, as IPv4. This value must already exist within
    /// the DigitalOcean account.
    pub ip: IpAddr,
    /// The numeric ID for the Droplet to which the IP address should be assigned.
    pub droplet_id: u32,
}

impl FloatingIp {
    /// Attaches the Floating IP to the Droplet specified by [FloatingIp::droplet_id].
    /// Requires that the Floating IP already exists.
    pub async fn assign(&self) -> Result<()> {
        let api_key =
            env::var("DIGITALOCEAN_API_TOKEN").context("DIGITALOCEAN_API_TOKEN not set.")?;
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
