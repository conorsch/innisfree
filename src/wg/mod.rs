//! Functions for managing Wireguard connections.
//! Includes methods for generating keypairs ([`WireguardKeypair::new`]),
//! for configuring interfaces ([WireguardHost]),

use anyhow::{anyhow, Context, Result};
use base64::Engine as _;
use std::io::Write as _;
use std::net::IpAddr;
use std::path::Path;

use crate::config::{make_config_dir, ServicePort};
use crate::net::generate_unused_subnet;
use serde::Serialize;
use x25519_dalek::{PublicKey, StaticSecret};

mod runtime;
pub use runtime::LocalWg;

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
    /// Generate a new Curve25519 keypair for use as a Wireguard identity.
    pub fn new() -> Result<WireguardKeypair> {
        let secret = StaticSecret::random_from_rng(rand_core::OsRng);
        let public = PublicKey::from(&secret);
        Ok(WireguardKeypair {
            private: encode_key(secret.as_bytes()),
            public: encode_key(public.as_bytes()),
        })
    }

    /// Returns the private key as a 32-byte array, ready for use with
    /// `x25519_dalek::StaticSecret::from` or boringtun's UAPI.
    pub fn private_bytes(&self) -> Result<[u8; 32]> {
        decode_key(&self.private).context("failed to decode wg private key")
    }

    /// Returns the public key as a 32-byte array.
    pub fn public_bytes(&self) -> Result<[u8; 32]> {
        decode_key(&self.public).context("failed to decode wg public key")
    }
}

/// Encode a 32-byte Wireguard key to standard base64 (no padding stripped),
/// matching the format produced by `wg genkey` / `wg pubkey`.
fn encode_key(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode a base64-encoded 32-byte Wireguard key.
fn decode_key(s: &str) -> Result<[u8; 32]> {
    let raw = base64::engine::general_purpose::STANDARD.decode(s.trim())?;
    raw.as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 32-byte key, got {}", raw.len()))
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
        let wg_template = include_str!("../../files/wg0.conf.j2");
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
        let wg_template = include_str!("../../files/wg0.conf.j2");
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
        f.write_all(self.config_with_services(services)?.as_bytes())?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
/// Controller class for creating both ends of a Wireguard tunnel.
/// Generates keypairs for local and remote interfaces.
/// Generates configuration files for both interfaces.
///
/// IPs and names live on the embedded [`WireguardDevice`]s — they used
/// to be duplicated as flat fields here, which made it possible for the
/// two copies to diverge.
pub struct WireguardManager {
    /// Wireguard configuration for local interface.
    pub local_device: WireguardDevice,
    /// Wireguard configuration for remote interface.
    pub remote_device: WireguardDevice,
}

impl WireguardManager {
    /// Create a new controller class, based on `service_name`.
    pub fn new(service_name: &str) -> Result<WireguardManager> {
        let wg_subnet = generate_unused_subnet()?;
        let s = wg_subnet.hosts().collect::<Vec<IpAddr>>();

        // Decouple the kernel-visible interface name from `service_name`:
        // user-provided names + the `innisfree-` / `-local` adornments would
        // routinely overflow Linux's IFNAMSIZ (15). Pick a short, fixed-shape
        // name (`innisfree<N>`) where N is the smallest integer not already
        // taken by an existing interface. Service identity stays in
        // `service_name` and the per-tunnel config dir.
        let local_name = pick_local_iface_name()?;
        let local_host = WireguardHost {
            name: local_name.clone(),
            address: s[0],
            endpoint: None,
            listenport: 0,
            keypair: WireguardKeypair::new()?,
        };

        let remote_name = format!("innisfree-{}-remote", service_name);
        let remote_host = WireguardHost {
            name: remote_name.clone(),
            address: s[1],
            endpoint: None,
            listenport: WIREGUARD_LISTEN_PORT,
            keypair: WireguardKeypair::new()?,
        };

        Ok(WireguardManager {
            local_device: WireguardDevice {
                name: local_name,
                interface: local_host.clone(),
                peer: remote_host.clone(),
            },
            remote_device: WireguardDevice {
                name: remote_name,
                interface: remote_host,
                peer: local_host,
            },
        })
    }
}

/// Pick the lowest unused `innisfree<N>` interface name. The pattern is
/// always 10–11 characters, well under IFNAMSIZ (15), regardless of what
/// the user named their tunnel.
///
/// Existence is detected via `/sys/class/net/<name>` rather than a netlink
/// dump — `/sys/class/net` is world-readable on every modern Linux and
/// keeps this function dep-free and callable from `WireguardManager::new`
/// (which is sync). Stale interfaces left behind by a crashed prior run
/// will simply be skipped over until the user `ip link delete`s them; with
/// 100 slots that's plenty of headroom in practice.
fn pick_local_iface_name() -> Result<String> {
    const MAX_SLOTS: u32 = 100;
    for n in 0..MAX_SLOTS {
        let candidate = format!("innisfree{n}");
        if !Path::new(&format!("/sys/class/net/{candidate}")).exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "no free interface name in innisfree[0..{MAX_SLOTS}); \
         clean up stale ones with `sudo ip link delete <name>`"
    ))
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
        // Hardcoded test vector, matches the output of `wg genkey | wg pubkey`.
        let privkey_b64 = "yPgz26A4S6RcniNaikFZrc0C0SyCW1moXmDP7AMeimE=";
        let secret = StaticSecret::from(decode_key(privkey_b64)?);
        let public = encode_key(PublicKey::from(&secret).as_bytes());
        assert_eq!(public, "ISRq2SHZQDnSfV0VlmMEP4MbwfExE/iNHzthMQ7eNmY=");
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
        assert_eq!(mgr.local_device.interface.address, local_ip);
        let remote_ip: IpAddr = "10.50.0.2".parse()?;
        assert_eq!(mgr.remote_device.interface.address, remote_ip);
        Ok(())
    }
}
