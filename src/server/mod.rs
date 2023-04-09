//! Abstract representation of remote server.
//! Designed to be modular in terms of providers, but really
//! only supports DigitalOcean. The abstract struct
//! is [InnisfreeServer], but underneath it assumes implementation
//! as a DigitalOcean Droplet.

use anyhow::Result;
use std::net::IpAddr;

use crate::config::ServicePort;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;

pub mod cloudinit;
pub mod digitalocean;

use self::cloudinit::generate_user_data;
use self::digitalocean::floating_ip::FloatingIp;
use self::digitalocean::server::Droplet;

/// Manager class, wraps a cloudserver VM type, such as Droplet,
/// to make it a bit easier to work with. Bootstraps the necessary keypairs
/// for services like SSH (both client and keyserver need keypairs), and Wireguard.
#[derive(Debug)]
pub struct InnisfreeServer {
    /// List of `ServicePort`s to manage connections for.
    pub services: Vec<ServicePort>,
    /// SSH keypair for managing client-side SSH connections.
    pub ssh_client_keypair: SshKeypair,
    /// SSH keypair for identifying remote SSH server identity.
    pub ssh_server_keypair: SshKeypair,
    // wg_mgr: WireguardManager,
    /// Server implementation. Only supports DigitalOcean, so `Droplet`.
    droplet: Droplet,
    // name: String,
}

impl InnisfreeServer {
    /// Create new [InnisfreeServer]. Requires a name for the service,
    /// a list of `ServicePort`s, and a [WireguardManager].
    pub async fn new(
        name: &str,
        services: Vec<ServicePort>,
        wg_mgr: WireguardManager,
    ) -> Result<InnisfreeServer> {
        // Initialize variables outside struct, so we'll need to pass them around
        let ssh_client_keypair = SshKeypair::new("client")?;
        let ssh_server_keypair = SshKeypair::new("server")?;
        let user_data =
            generate_user_data(&ssh_client_keypair, &ssh_server_keypair, &wg_mgr, &services)
                .await?;
        let droplet = Droplet::new(name, &user_data, ssh_client_keypair.public.to_owned()).await?;
        Ok(InnisfreeServer {
            services,
            ssh_client_keypair,
            ssh_server_keypair,
            // wg_mgr,
            droplet,
            // name: name.to_string(),
        })
    }
    /// Returns the IPv4 address for the remote server. Used for both
    /// SSH connections and the remote Wireguard peer interface.
    // TODO maybe this should be a trait
    pub fn ipv4_address(&self) -> IpAddr {
        let droplet = &self.droplet;
        droplet.ipv4_address()
    }
    /// Attaches a reserved IP to the remote server. Makes it easier
    /// to use DNS, since the record needs to be updated only once,
    /// and the IP address can be reused repeatedly on multiple hosts after that.
    pub async fn assign_floating_ip(&self, floating_ip: IpAddr) -> Result<()> {
        let f = FloatingIp {
            ip: floating_ip,
            droplet_id: self.droplet.id,
        };
        f.assign().await
    }
    /// Destroy the cloud server backing the remote end of the Wireguard tunnel.
    pub async fn destroy(&self) -> Result<()> {
        // Destroys backing droplet
        // TODO: destructions should be provider agnostic.
        self.droplet.destroy().await
    }
}
