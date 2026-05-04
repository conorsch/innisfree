//! Logic for managing a remote server via the DigitalOcean cloud provider.
//! Ideally the cloud provider logic would be generalized, but right now
//! DigitalOcean is the only supported provider.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use anyhow::{anyhow, Context, Result};
use std::net::IpAddr;
use std::time::Duration;

use crate::server::cloudinit::CloudConfig;
use crate::server::digitalocean::client::DoClient;
use crate::server::digitalocean::floating_ip::FloatingIp;
use crate::server::digitalocean::ssh_key::DigitalOceanSshKey;
use crate::server::{InnisfreeServer, ServerSpec};

/// The zone in which the resources will be created, e.g. `sfo2`.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Regions>.
pub const DO_REGION: &str = "sfo2";
/// The type of VM instance to create, e.g. `s-1vcpu-1gb`.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Sizes>.
pub const DO_SIZE: &str = "s-1vcpu-1gb";
/// The OS choice for to base the Droplet on. Defaults to most recent Debian Stable.
/// See docs for more info: <https://docs.digitalocean.com/reference/api/api-reference/#tag/Images>.
pub const DO_IMAGE: &str = "debian-13-x64";

/// Lifecycle state of a Droplet, as reported by the DigitalOcean API.
///
/// Known values come from the API reference; an unknown variant is preserved
/// via `#[serde(other)]` so a future state added by DO doesn't fail
/// deserialization mid-poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DropletStatus {
    /// Just created; networking info has not yet been assigned.
    New,
    /// Booted and reachable; networking info is populated.
    Active,
    /// Powered off.
    Off,
    /// Archived (long-term off); networking info is no longer reserved.
    Archive,
    /// Any state DO adds in the future that this binary doesn't recognize.
    #[serde(other)]
    Unknown,
}

/// One IP-address-bearing entry inside a Droplet's `networks.{v4,v6}` array.
///
/// We deserialize only the two fields we consume; serde ignores the rest
/// (`netmask`, `gateway`), which is helpful because their JSON shape differs
/// between v4 (string) and v6 (number).
#[derive(Debug, Deserialize)]
struct Network {
    ip_address: IpAddr,
    #[serde(rename = "type")]
    kind: NetworkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum NetworkKind {
    Public,
    Private,
}

/// Typed shape of the DO API's `droplet.networks` object. Replaces the
/// previous `HashMap<String, Vec<HashMap<String, String>>>` so callers no
/// longer have to index into stringly-typed maps to find the public IP.
///
/// We only project `v4` because nothing in this codebase consumes IPv6 yet;
/// add a `v6` field here when that changes.
#[derive(Debug, Deserialize, Default)]
pub(super) struct Networks {
    #[serde(default)]
    v4: Vec<Network>,
}

/// Wire-format projection of a Droplet, deserialized straight from the DO
/// API. Stays separate from [`Droplet`] so the domain type can hold a
/// non-`Option` [`DoClient`] without contorting serde.
///
/// Lives at this visibility because [`crate::server::digitalocean::client`]
/// is the only producer.
#[derive(Debug, Deserialize)]
pub(super) struct DropletDto {
    pub(super) id: u32,
    pub(super) status: DropletStatus,
    #[serde(default)]
    pub(super) networks: Networks,
}

/// Representation of a DigitalOcean Droplet, i.e. cloud VM. Constructed
/// only once the API reports `status="active"`; the status value isn't
/// retained because nothing downstream needs it.
#[derive(Debug)]
pub struct Droplet {
    /// Numeric ID, returned by API, to identify this Droplet.
    pub id: u32,
    /// Information about host networking, such as public and private
    /// interfaces and their corresponding IPv4/6 addresses. Use
    /// [Droplet::ipv4_address] to obtain an IP address easily.
    networks: Networks,
    // The API takes a list, but we only care about 1 key,
    // the generated one, so use that.
    /// Optional dynamically generated SSH keypair, stored in cloud,
    /// used for initial connection (and to suppress emails about root
    /// passwords on instance creation).
    // TODO: Make this mandatory, since it's automatically created anyway.
    ssh_pubkey: Option<DigitalOceanSshKey>,
    /// Authenticated DO client, used for any post-construction API calls
    /// (poll, destroy, assign-floating-ip).
    client: DoClient,
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
    /// The [crate::manager::TunnelManager] will handle automatically generating
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
    /// Provision a new DigitalOcean droplet for `spec`. Blocks until
    /// the API reports `status="active"` (~60 s typical).
    ///
    /// Called only by [`crate::server::digitalocean::provider::DigitalOceanProvider`];
    /// other call sites should go through the [`crate::server::Provider`]
    /// trait so they stay backend-agnostic.
    pub(super) async fn new(client: DoClient, spec: &ServerSpec) -> Result<Self> {
        tracing::debug!("Creating new DigitalOcean Droplet");
        let mut cc = CloudConfig::new(
            &spec.ssh_client_keypair,
            &spec.ssh_server_keypair,
            &spec.wg_mgr,
            &spec.services,
        )?;
        // Look up the SSH pubkeys associated with the cloud account, and
        // authorize them on the new host so operators can manage it as they
        // manage other machines.
        let account_keys = client.list_ssh_keys().await?;
        if account_keys.is_empty() {
            tracing::warn!("No SSH pubkeys found via API");
        }
        cc.authorize_ssh_keys(account_keys.into_iter().map(|k| k.public_key));
        let user_data: String = cc.try_into()?;
        let do_ssh_key = client
            .create_ssh_key(&spec.name, &spec.ssh_client_keypair.public)
            .await?;
        let ssh_keys: Vec<u32> = vec![do_ssh_key.id];
        // Build JSON request body, for sending to DigitalOcean API
        let droplet_config = DropletConfig {
            name: spec.name.clone(),
            user_data,
            ssh_keys,
            ..DropletConfig::new()
        };

        let dto = client.create_droplet(&droplet_config).await?;
        tracing::debug!("Server created, waiting for networking");
        wait_for_boot(&client, dto.id).await.map(|dto| Droplet {
            id: dto.id,
            networks: dto.networks,
            ssh_pubkey: Some(do_ssh_key),
            client,
        })
    }
}

/// Block until the API reports `status="active"`, at which point networking
/// info is populated. Returns the refreshed DTO so the caller can build a
/// final [`Droplet`] without keeping any stale state.
async fn wait_for_boot(client: &DoClient, id: u32) -> Result<DropletDto> {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        let dto = client
            .get_droplet(id)
            .await
            .context("polling droplet boot status")?;
        if dto.status == DropletStatus::Active {
            return Ok(dto);
        }
        tracing::info!("Server still booting, waiting...");
    }
}

#[async_trait]
impl InnisfreeServer for Droplet {
    /// Retrieves the public IPv4 address for the Droplet. Returns an error
    /// if the API response contains no public v4 network — typically because
    /// the droplet hasn't finished booting yet (see [`Droplet::wait_for_boot`]).
    fn ipv4_address(&self) -> Result<IpAddr> {
        self.networks
            .v4
            .iter()
            .find(|n| n.kind == NetworkKind::Public)
            .map(|n| n.ip_address)
            .ok_or_else(|| anyhow!("droplet {} has no public IPv4 network", self.id))
    }

    async fn assign_floating_ip(&self, floating_ip: IpAddr) -> Result<()> {
        let f = FloatingIp {
            ip: floating_ip,
            droplet_id: self.id,
        };
        f.assign(&self.client).await
    }

    /// Calls the API to destroy a droplet.
    async fn destroy(&self) -> Result<()> {
        if let Some(k) = &self.ssh_pubkey {
            k.destroy(&self.client).await?;
        } else {
            tracing::warn!("No API pubkey associated with droplet, not destroying");
        }
        self.client.destroy_droplet(self.id).await?;
        tracing::debug!("Droplet destroyed");
        Ok(())
    }
}
