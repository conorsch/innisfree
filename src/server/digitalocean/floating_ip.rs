//! Logic to assign a pre-existing Floating IP resource in DigitalOcean
//! to the Droplet used for managing the tunnel. Allows for DNS records
//! to remain unchanged, but the tunnel to be rebuilt ad-hoc.
use anyhow::Result;
use std::net::IpAddr;

use crate::server::digitalocean::client::DoClient;

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
    pub async fn assign(&self, client: &DoClient) -> Result<()> {
        tracing::debug!("Assigning floating IP to droplet...");
        client.assign_floating_ip(self.ip, self.droplet_id).await
    }
}
