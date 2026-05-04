//! Single-instance HTTP client for the DigitalOcean v2 API.
//!
//! Replaces the seven prior call sites that each rebuilt
//! [`reqwest::Client::new`] and re-read `DIGITALOCEAN_API_TOKEN`.
//! Construct once at startup via [`DoClient::from_env`] and pass by
//! clone — the inner [`reqwest::Client`] is internally `Arc`-shared,
//! so cloning is cheap.

use anyhow::{anyhow, Context, Result};
use reqwest::Client as HttpClient;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::env;
use std::net::IpAddr;

use crate::server::digitalocean::server::{DropletConfig, DropletDto};
use crate::server::digitalocean::ssh_key::DigitalOceanSshKey;

const DO_API_BASE: &str = "https://api.digitalocean.com/v2";

/// Authenticated handle to the DigitalOcean v2 API.
///
/// Owns the bearer token and the underlying HTTP client. Cheap to clone
/// (the inner `reqwest::Client` is `Arc`-shared) so callers can hand
/// it down to long-lived structs like [`crate::server::digitalocean::server::Droplet`]
/// without taking a reference.
#[derive(Debug, Clone)]
pub struct DoClient {
    http: HttpClient,
    token: String,
}

impl DoClient {
    /// Construct a client by reading `DIGITALOCEAN_API_TOKEN` from the
    /// environment. Fails fast if the var is missing — call this at
    /// startup so misconfiguration surfaces before any other work.
    pub fn from_env() -> Result<Self> {
        let token =
            env::var("DIGITALOCEAN_API_TOKEN").context("DIGITALOCEAN_API_TOKEN env var not set")?;
        Ok(Self {
            http: HttpClient::new(),
            token,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{DO_API_BASE}{path}")
    }

    /// DO wraps every resource in a `{ "<name>": {...} }` envelope.
    /// Pull out the named field and deserialize it, surfacing a clear
    /// error if the envelope shape ever changes.
    async fn unwrap_envelope<T: DeserializeOwned>(resp: reqwest::Response, key: &str) -> Result<T> {
        let mut envelope: serde_json::Value = resp.json().await?;
        let body = envelope
            .get_mut(key)
            .map(|v| v.take())
            .ok_or_else(|| anyhow!("DO response missing '{key}' field"))?;
        serde_json::from_value(body).with_context(|| format!("decoding '{key}' from DO response"))
    }

    /// `POST /v2/droplets` — create a new droplet.
    pub(super) async fn create_droplet(&self, cfg: &DropletConfig) -> Result<DropletDto> {
        let resp = self
            .http
            .post(self.url("/droplets"))
            .bearer_auth(&self.token)
            .json(cfg)
            .send()
            .await?
            .error_for_status()?;
        Self::unwrap_envelope(resp, "droplet").await
    }

    /// `GET /v2/droplets/{id}` — refresh a droplet's networking/status.
    pub(super) async fn get_droplet(&self, id: u32) -> Result<DropletDto> {
        let resp = self
            .http
            .get(self.url(&format!("/droplets/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?;
        Self::unwrap_envelope(resp, "droplet").await
    }

    /// `DELETE /v2/droplets/{id}` — destroy the droplet.
    pub(super) async fn destroy_droplet(&self, id: u32) -> Result<()> {
        self.http
            .delete(self.url(&format!("/droplets/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .context("destroying droplet")?;
        Ok(())
    }

    /// `POST /v2/floating_ips/{ip}/actions` — assign a reserved IP to a droplet.
    pub(super) async fn assign_floating_ip(&self, ip: IpAddr, droplet_id: u32) -> Result<()> {
        let body = json!({ "type": "assign", "droplet_id": droplet_id });
        self.http
            .post(self.url(&format!("/floating_ips/{ip}/actions")))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("assigning floating IP")?;
        Ok(())
    }

    /// `GET /v2/account/keys` — list all SSH pubkeys on the account.
    pub(super) async fn list_ssh_keys(&self) -> Result<Vec<DigitalOceanSshKey>> {
        let resp = self
            .http
            .get(self.url("/account/keys"))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()?;
        Self::unwrap_envelope(resp, "ssh_keys").await
    }

    /// `POST /v2/account/keys` — register a new SSH pubkey.
    pub(super) async fn create_ssh_key(
        &self,
        name: &str,
        public_key: &str,
    ) -> Result<DigitalOceanSshKey> {
        let body = json!({ "name": name, "public_key": public_key });
        let resp = self
            .http
            .post(self.url("/account/keys"))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        Self::unwrap_envelope(resp, "ssh_key").await
    }

    /// `DELETE /v2/account/keys/{id}` — remove a registered SSH pubkey.
    pub(super) async fn destroy_ssh_key(&self, id: u32) -> Result<()> {
        self.http
            .delete(self.url(&format!("/account/keys/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?
            .error_for_status()
            .context("destroying SSH key")?;
        Ok(())
    }
}
