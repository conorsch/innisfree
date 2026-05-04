//! Logic to create a new, ephemeral SSH keypair in DigitalOcean.
//! Adding a new SSH key on instance creation prevents emails on
//! instance creation, providing a root pw.

use anyhow::Result;
use serde::Deserialize;

use crate::server::digitalocean::client::DoClient;

#[derive(Clone, Debug, Deserialize)]
/// Representation of an SSH public key, as defined by the DigitalOcean API.
/// For more information, see
/// <https://docs.digitalocean.com/reference/api/api-reference/#tag/SSH-Keys>.
pub struct DigitalOceanSshKey {
    /// Public key material, in ED25519 format, for the SSH keypair.
    pub public_key: String,
    /// Numeric ID, created automatically by the DigitalOcean API, for this
    /// specific key. The ID can be used during Droplet creation requests
    /// to ensure a public key is present.
    pub id: u32,
}

impl DigitalOceanSshKey {
    /// Delete the DigitalOceanSshKey via the API. Borrows a [`DoClient`]
    /// rather than carrying its own — the parent
    /// [`crate::server::digitalocean::server::Droplet`] already holds the
    /// authoritative client.
    pub async fn destroy(&self, client: &DoClient) -> Result<()> {
        tracing::debug!("Deleting SSH keypair from DigitalOcean...");
        client.destroy_ssh_key(self.id).await
    }
}
