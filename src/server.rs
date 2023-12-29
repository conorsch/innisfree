//! Abstract representation of remote server.
//! Designed to be modular in terms of providers, but really
//! only supports DigitalOcean. The abstract struct
//! is [InnisfreeServer], but underneath it assumes implementation
//! as a DigitalOcean Droplet.

use anyhow::Result;
use async_trait::async_trait;
use std::net::IpAddr;

use crate::config::ServicePort;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;

pub mod cloudinit;
pub mod digitalocean;

/// Manager class, wraps a cloudserver VM type, such as Droplet,
/// to make it a bit easier to work with. Bootstraps the necessary keypairs
/// for services like SSH (both client and keyserver need keypairs), and Wireguard.
#[async_trait]
pub trait InnisfreeServer {
    /// Create new [InnisfreeServer]. Requires a name for the service,
    /// a list of `ServicePort`s, and a [WireguardManager].
    async fn new(
        name: &str,
        services: Vec<ServicePort>,
        wg_mgr: WireguardManager,
        ssh_client_keypair: &SshKeypair,
        ssh_server_keypair: &SshKeypair,
    ) -> Result<Self>
    where
        Self: Sized;

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
