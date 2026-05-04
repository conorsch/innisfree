//! [`Provider`] implementation for DigitalOcean.
//!
//! Holds the authenticated [`DoClient`] (constructed once at startup)
//! and hands it to [`Droplet::new`] each time a server is requested.

use anyhow::Result;
use async_trait::async_trait;

use crate::server::digitalocean::client::DoClient;
use crate::server::digitalocean::server::Droplet;
use crate::server::{InnisfreeServer, Provider, ServerSpec};

/// DigitalOcean factory. Construct once with [`DoClient::from_env`] and
/// pass to [`crate::manager::TunnelManager::new`].
pub struct DigitalOceanProvider {
    client: DoClient,
}

impl DigitalOceanProvider {
    /// Wrap a configured [`DoClient`] as a [`Provider`].
    pub fn new(client: DoClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Provider for DigitalOceanProvider {
    async fn create(&self, spec: &ServerSpec) -> Result<Box<dyn InnisfreeServer>> {
        let droplet = Droplet::new(self.client.clone(), spec).await?;
        Ok(Box::new(droplet))
    }
}
