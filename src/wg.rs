//! Functions for managing Wireguard connections.
//! Includes methods for generating keypairs ([`WireguardKeypair::new`]),
//! for configuring interfaces ([WireguardHost]),

use anyhow::{Context, Result};
use std::io::prelude::*;
use std::net::IpAddr;
use std::process::{Command, Stdio};
use std::str;

use crate::config::{make_config_dir, ServicePort};
use crate::net::generate_unused_subnet;
use serde::Serialize;

const WIREGUARD_LISTEN_PORT: i32 = 51820;

#[derive(Debug, Serialize, Clone)]
/// Contains the public and private key material
/// for a Wireguard ED25519 keypair.
pub struct WireguardKeypair {
    /// Private key material.
    private: String,
    /// Public key material.
    public: String,
}

impl WireguardKeypair {
    /// Generate a new ED25519 keypair for use as a Wireguard identity.
    pub fn new() -> Result<WireguardKeypair> {
        let privkey = generate_wireguard_privkey()?;
        let pubkey = derive_wireguard_pubkey(&privkey)?;
        Ok(WireguardKeypair {
            private: privkey,
            public: pubkey,
        })
    }
}

#[derive(Debug, Serialize, Clone)]
/// Represents a Wireguard that can be peered with.
pub struct WireguardHost {
    /// Human-readable name for peer on the Wireguard network.
    pub name: String,
    /// IP address within the Wireguard network for exclusive use by this host.
    pub address: IpAddr,
    /// Publicly accessible IP address to allow peers to connect over Wireguard.
    /// Optional, because only the remote host will have an Endpoint.
    pub endpoint: Option<IpAddr>,
    /// The UDP port on which Wireguard will listen for incoming peer traffic.
    /// This port is not related to [crate::config::ServicePort].
    pub listenport: i32,
    /// An ED25519 keypair defining the identity of this [WireguardHost].
    /// Its public key will be referred to in peers' configs, and its private
    /// key will be used to initialize the interface.
    pub keypair: WireguardKeypair,
}

#[derive(Debug, Serialize, Clone)]
/// Represents a network device for handling Wireguard traffic.
/// Must include remote and local identities in the form of `WireguardHost`.
pub struct WireguardDevice {
    /// Human-readable name for this device.
    pub name: String,
    /// Representation of localhost as a [WireguardHost].
    pub interface: WireguardHost,
    /// Representation of remote peer as a [WireguardHost].
    pub peer: WireguardHost,
}

impl WireguardDevice {
    /// Returns contents of an INI config file for Wireguard.
    /// This file constitutes the entirety of the Wireguard interface configuration,
    /// for use with `wg-quick`, and is usually referred to in Wireguard documentation
    /// as `wg0.conf`. In our case, on disk it is usually called `innisfree.conf`.
    /// In practice, this config file is used by the local end of the Innisfree tunnel.
    pub fn config(&self) -> Result<String> {
        let wg_template = include_str!("../files/wg0.conf.j2");
        let mut context = tera::Context::new();
        context.insert("wireguard_device", &self);
        // Firewall rules are mostly important from client side,
        // so allow rules to be ignored
        let empty_rules: Vec<ServicePort> = Vec::new();
        context.insert("services", &empty_rules);
        // Disable autoescaping, since it breaks wg key contents
        tera::Tera::one_off(wg_template, &context, false)
            .context("Failed to write wireguard config")
    }

    /// Returns a specially formed Wireguard INI config file,
    /// that includes firewall rules restrictions for the services
    /// being proxied.
    pub fn config_with_services(&self, services: &[ServicePort]) -> Result<String> {
        let wg_template = include_str!("../files/wg0.conf.j2");
        let mut context = tera::Context::new();
        context.insert("wireguard_device", &self);
        context.insert("services", &services);
        // Disable autoescaping, since it breaks wg key contents
        tera::Tera::one_off(wg_template, &context, false)
            .context("Failed to write wireguard config for multiple services")
    }

    /// Save the config file to disk, within the configuration directory for project state.
    /// This method is only appropriate for the local end of the Innisfree tunnel.
    pub fn write_locally(&self, service_name: &str, services: &[ServicePort]) -> Result<()> {
        let wg_config_path = make_config_dir(service_name)?.join(format!("{}.conf", service_name));
        let mut f = std::fs::File::create(&wg_config_path)?;
        f.write_all(&self.config_with_services(services)?.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
/// Controller class for creating both ends of a Wireguard tunnel.
/// Generates keypairs for local and remote interfaces.
/// Generates configuration files for both interfaces.
pub struct WireguardManager {
    /// IP address of the local Wireguard interface.
    pub wg_local_ip: IpAddr,
    // wg_local_name: String,
    // wg_local_host: WireguardHost,
    /// Wireguard configuration for local interface.
    pub wg_local_device: WireguardDevice,

    /// IP address of the remote Wireguard interface.
    pub wg_remote_ip: IpAddr,
    // wg_remote_name: String,
    // wg_remote_host: WireguardHost,
    /// Wireguard configuration for remote interface.
    pub wg_remote_device: WireguardDevice,
}

impl WireguardManager {
    /// Create a new controller class, based on `service_name`.
    pub fn new(service_name: &str) -> Result<WireguardManager> {
        let wg_subnet = generate_unused_subnet()?;
        let s = wg_subnet.hosts().collect::<Vec<IpAddr>>();

        let wg_local_ip = s[0];
        let wg_local_name = format!("innisfree-{}-local", service_name);
        let wg_local_keypair = WireguardKeypair::new()?;
        let wg_local_host = WireguardHost {
            name: wg_local_name.to_owned(),
            address: wg_local_ip,
            endpoint: None,
            listenport: 0,
            keypair: wg_local_keypair,
        };

        let wg_remote_ip = s[1];
        let wg_remote_name = format!("innisfree-{}-remote", service_name);
        let wg_remote_keypair = WireguardKeypair::new()?;
        let wg_remote_host = WireguardHost {
            name: wg_remote_name.to_owned(),
            address: wg_remote_ip,
            endpoint: None,
            listenport: WIREGUARD_LISTEN_PORT,
            keypair: wg_remote_keypair,
        };

        let wg_local_device = WireguardDevice {
            name: wg_local_name,
            interface: wg_local_host.clone(),
            peer: wg_remote_host.clone(),
        };
        let wg_remote_device = WireguardDevice {
            name: wg_remote_name,
            interface: wg_remote_host,
            peer: wg_local_host,
        };

        Ok(WireguardManager {
            wg_local_ip,
            // wg_local_name,
            // wg_local_host,
            wg_local_device,

            wg_remote_ip,
            // wg_remote_name,
            // wg_remote_host,
            wg_remote_device,
        })
    }
}

/// Create a new ED25519 private key via ``wg genkey``.
fn generate_wireguard_privkey() -> Result<String> {
    // Call out to "wg genkey" and collect output.
    // Ideally we'd generate these values in pure Rust, but
    // calling out to wg as a first draft.
    let privkey_cmd = std::process::Command::new("wg")
        .args(["genkey"])
        .output()
        .context("Failed to generate Wireguard keypair")?;
    let privkey: String = str::from_utf8(&privkey_cmd.stdout)?.trim().to_string();
    Ok(privkey)
}

/// Return an ED25519 public key from an ED25519 private key,
/// via ``wg pubkey``.
fn derive_wireguard_pubkey(privkey: &str) -> Result<String> {
    // Open a pipe to 'wg genkey', to pass in the privkey
    let pubkey_cmd = Command::new("wg")
        .args(["pubkey"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // Write wg privkey to stdin on pubkey process
    pubkey_cmd
        .stdin
        .ok_or(anyhow::anyhow!("failed to open stdin on wg pubkey command"))?
        .write_all(privkey.as_bytes())?;

    let mut pubkey = String::new();
    pubkey_cmd
        .stdout
        .ok_or(anyhow::anyhow!(
            "failed to open stdout on wg pubkey command"
        ))?
        .read_to_string(&mut pubkey)?;

    pubkey = pubkey.trim().to_string();
    Ok(pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_generation() -> anyhow::Result<()> {
        let wg_hosts = _generate_hosts()?;
        let wg_device = WireguardDevice {
            name: "foo1".to_string(),
            interface: wg_hosts[0].clone(),
            peer: wg_hosts[1].clone(),
        };
        let wg_config = wg_device.config()?;
        assert!(wg_config.contains("Interface"));
        assert!(wg_config.contains("PrivateKey = "));

        assert!(!wg_config.contains(&wg_hosts[0].keypair.public));
        assert!(wg_config.contains(&wg_hosts[0].keypair.private));

        assert!(wg_config.contains(&wg_hosts[1].keypair.public));
        assert!(!wg_config.contains(&wg_hosts[1].keypair.private));

        // Slashes '/' will be rendered as hex value &#x2F if formatting is broken
        assert!(!wg_config.contains("&#x2F"));
        assert!(!wg_config.contains(r"&#x2F"));

        Ok(())
    }

    // Helper function for reusable structs
    fn _generate_hosts() -> Result<Vec<WireguardHost>> {
        let kp1 = WireguardKeypair::new()?;
        let h1 = WireguardHost {
            name: "foo1".to_string(),
            address: "127.0.0.1".parse()?,
            endpoint: Some("1.1.1.1".parse()?),
            listenport: 80,
            keypair: kp1,
        };
        let kp2 = WireguardKeypair::new()?;
        let h2 = WireguardHost {
            name: "foo2".to_string(),
            address: "127.0.0.1".parse()?,
            endpoint: None,
            listenport: 80,
            keypair: kp2,
        };
        let wg_hosts: Vec<WireguardHost> = vec![h1, h2];
        Ok(wg_hosts)
    }

    #[test]
    fn host_generation() -> anyhow::Result<()> {
        let wg_hosts = _generate_hosts()?;
        assert_eq!(wg_hosts[0].name, "foo1");
        assert_eq!(wg_hosts[1].name, "foo2");
        Ok(())
    }

    #[test]
    fn device_generation() -> anyhow::Result<()> {
        let wg_hosts = _generate_hosts()?;
        let wg_device = WireguardDevice {
            name: "foo".to_string(),
            interface: wg_hosts[0].clone(),
            peer: wg_hosts[1].clone(),
        };
        assert_eq!(wg_device.name, "foo");
        assert_eq!(wg_hosts[0].name, "foo1");
        Ok(())
    }

    #[test]
    fn host_cloning() -> anyhow::Result<()> {
        let wg_hosts = _generate_hosts()?;
        let wg_h1 = &wg_hosts[0];
        let wg_h2 = &wg_hosts[1];
        let wg_device = WireguardDevice {
            name: "foo".to_string(),
            interface: wg_h1.clone(),
            peer: wg_h2.clone(),
        };
        assert_eq!(wg_device.name, "foo");
        assert_eq!(wg_hosts[0].name, "foo1");
        assert_eq!(wg_device.interface.keypair.public, wg_h1.keypair.public);
        assert_eq!(wg_device.interface.keypair.private, wg_h1.keypair.private);
        Ok(())
    }

    #[test]
    fn device_cloning() -> anyhow::Result<()> {
        let wg_hosts = _generate_hosts()?;
        let wg_h1 = &wg_hosts[0];
        let wg_h2 = &wg_hosts[1];
        let wg_device = WireguardDevice {
            name: "foo".to_string(),
            interface: wg_h1.clone(),
            peer: wg_h2.clone(),
        };

        let wg_device2 = wg_device.clone();
        assert_eq!(
            wg_device.interface.keypair.public,
            wg_device2.interface.keypair.public
        );
        assert_eq!(
            wg_device.interface.keypair.private,
            wg_device2.interface.keypair.private
        );
        Ok(())
    }

    #[test]
    fn pubkey_generation() -> anyhow::Result<()> {
        // Use hardcoded privkey value, to compare results with
        // 'wg genkey | wg pubkey'
        let privkey = String::from("yPgz26A4S6RcniNaikFZrc0C0SyCW1moXmDP7AMeimE=");
        let expected_pubkey = "ISRq2SHZQDnSfV0VlmMEP4MbwfExE/iNHzthMQ7eNmY=";
        tracing::debug!("Expecting pubkey: {}", expected_pubkey);
        let pubkey = derive_wireguard_pubkey(&privkey)?;
        tracing::debug!("Found pubkey: {}", pubkey);
        assert_eq!(pubkey, "ISRq2SHZQDnSfV0VlmMEP4MbwfExE/iNHzthMQ7eNmY=");
        Ok(())
    }

    #[test]
    fn key_generation() -> anyhow::Result<()> {
        let kp = WireguardKeypair::new()?;
        assert!(!kp.public.ends_with('\n'));
        assert!(!kp.private.ends_with('\n'));
        // Slashes '/' will be rendered as hex value &#x2F if formatting is broken
        // Confirming they're NOT in the raw key parts, looks like they slipped
        // in during development in the tera template output.
        assert!(!kp.public.contains("&#x2F"));
        assert!(!kp.public.contains(r"&#x2F"));
        assert!(!kp.private.contains("&#x2F"));
        assert!(!kp.private.contains(r"&#x2F"));
        Ok(())
    }

    #[test]
    fn create_manager() -> anyhow::Result<()> {
        // We'll assume the test host has no tunnels. The first
        // tunnel configured should be 10.50.0.1/30, assuming those
        // IP addrs are available on the system.
        let mgr = WireguardManager::new("foo-service")?;
        let local_ip: IpAddr = "10.50.0.1".parse()?;
        assert!(mgr.wg_local_ip == local_ip);
        let remote_ip: IpAddr = "10.50.0.2".parse()?;
        assert!(mgr.wg_remote_ip == remote_ip);
        Ok(())
    }
}
