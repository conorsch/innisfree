//! Abstract representation of a remote server, plus the [`Provider`]
//! factory that knows how to bring one up.
//!
//! Today only DigitalOcean is implemented (see
//! [`crate::server::digitalocean::provider::DigitalOceanProvider`]),
//! but the trait shapes are deliberately minimal so a second backend
//! (Hetzner, Linode, …) only needs a new [`Provider`] impl plus a CLI
//! flag to select it.

use anyhow::Result;
use async_trait::async_trait;
use std::net::IpAddr;

use crate::config::ServicePort;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;

pub mod cloudinit;
pub mod digitalocean;

/// The provider-agnostic recipe for "stand up a server for this tunnel."
///
/// Owned data so the eventual [`Provider::create`] future can be `'static`
/// without macro lifetime gymnastics; clones here are cheap (two
/// short Strings inside each [`SshKeypair`], the wg manager is a few
/// IPs and keypairs).
pub struct ServerSpec {
    /// Hostname / DO droplet name; usually the cleaned tunnel name.
    pub name: String,
    /// Service ports to publish on the remote ingress.
    pub services: Vec<ServicePort>,
    /// Wireguard manager, used by the provider to render the remote
    /// `wg0.conf` and the cloud-init.
    pub wg_mgr: WireguardManager,
    /// SSH keypair the operator (and innisfree itself) will log in with.
    pub ssh_client_keypair: SshKeypair,
    /// SSH keypair the remote sshd identifies itself with — pinned in
    /// the operator's `known_hosts` to defeat first-use MITM.
    pub ssh_server_keypair: SshKeypair,
}

/// Factory for building cloud servers. Implementations encapsulate
/// provider-specific credentials and API plumbing (e.g. a held
/// [`crate::server::digitalocean::client::DoClient`]).
#[async_trait]
pub trait Provider: Send + Sync {
    /// Bring up a new server matching `spec`, blocking until it is
    /// network-reachable. Returns a type-erased handle so callers can
    /// remain provider-agnostic.
    async fn create(&self, spec: &ServerSpec) -> Result<Box<dyn InnisfreeServer>>;
}

/// Handle to a running cloud server. Construction goes through
/// [`Provider::create`] — this trait only describes the operations a
/// caller can perform on an already-live server.
#[async_trait]
pub trait InnisfreeServer: Send + Sync {
    /// Returns the IPv4 address for the remote server. Used for both
    /// SSH connections and the remote Wireguard peer interface.
    fn ipv4_address(&self) -> Result<IpAddr>;

    /// Attaches a reserved IP to the remote server. Makes it easier
    /// to use DNS, since the record needs to be updated only once,
    /// and the IP address can be reused repeatedly on multiple hosts after that.
    async fn assign_floating_ip(&self, floating_ip: IpAddr) -> Result<()>;

    /// Destroy the cloud server backing the remote end of the Wireguard tunnel.
    async fn destroy(&self) -> Result<()>;
}
