use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::str;

use crate::config::{make_config_dir, InnisfreeError, ServicePort};

extern crate tera;

extern crate serde;
use serde::Serialize;

// Cutting corners here. IP addresses should be customizable,
// and be a valid /30.
const WIREGUARD_LISTEN_PORT: i32 = 51820;
pub const WIREGUARD_LOCAL_IP: &str = "10.50.0.1";
pub const WIREGUARD_REMOTE_IP: &str = "10.50.0.2";

#[derive(Debug, Serialize, Clone)]
pub struct WireguardKeypair {
    private: String,
    public: String,
}

impl WireguardKeypair {
    pub fn new() -> Result<WireguardKeypair, InnisfreeError> {
        let privkey = match generate_wireguard_privkey() {
            Ok(p) => p,
            Err(_) => {
                return Err(InnisfreeError::CommandFailure {
                    msg: "Failed to generate wireguard keypair".to_owned(),
                })
            }
        };
        let pubkey = derive_wireguard_pubkey(&privkey)?;
        Ok(WireguardKeypair {
            private: privkey,
            public: pubkey,
        })
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct WireguardHost {
    pub name: String,
    pub address: String,
    pub endpoint: String,
    pub listenport: i32,
    pub keypair: WireguardKeypair,
}

#[derive(Debug, Serialize, Clone)]
pub struct WireguardDevice {
    pub name: String,
    pub interface: WireguardHost,
    pub peer: WireguardHost,
}

impl WireguardDevice {
    // Returns contents of an INI config file for WG, e.g. 'wg0.conf' in docs.
    pub fn config(&self) -> String {
        let wg_template = include_str!("../files/wg0.conf.j2");
        let mut context = tera::Context::new();
        context.insert("wireguard_device", &self);
        // Firewall rules are mostly important from client side,
        // so allow rules to be ignored
        let empty_rules: Vec<ServicePort> = Vec::new();
        context.insert("services", &empty_rules);
        // Disable autoescaping, since it breaks wg key contents
        tera::Tera::one_off(wg_template, &context, false).unwrap()
    }

    pub fn config_with_services(&self, services: Vec<ServicePort>) -> String {
        let wg_template = include_str!("../files/wg0.conf.j2");
        let mut context = tera::Context::new();
        context.insert("wireguard_device", &self);
        context.insert("services", &services);
        // Disable autoescaping, since it breaks wg key contents
        tera::Tera::one_off(wg_template, &context, false).unwrap()
    }

    pub fn write_locally(&self, services: Vec<ServicePort>) {
        let mut wg_config_path = std::path::PathBuf::from(make_config_dir());
        wg_config_path.push("innisfree.conf");
        let mut f = std::fs::File::create(&wg_config_path).unwrap();
        let wg_config = &self.config_with_services(services);
        f.write_all(wg_config.as_bytes()).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct WireguardManager {
    pub wg_local_ip: String,
    wg_local_name: String,
    wg_local_host: WireguardHost,
    pub wg_local_device: WireguardDevice,

    pub wg_remote_ip: String,
    wg_remote_name: String,
    wg_remote_host: WireguardHost,
    pub wg_remote_device: WireguardDevice,
}

impl WireguardManager {
    pub fn new() -> Result<WireguardManager, InnisfreeError> {
        let wg_local_ip = WIREGUARD_LOCAL_IP.to_string();
        let wg_local_name = "innisfree_local".to_string();
        let wg_local_keypair = WireguardKeypair::new()?;
        let wg_local_host = WireguardHost {
            name: wg_local_name.clone(),
            address: wg_local_ip.clone(),
            endpoint: "".to_string(),
            listenport: 0,
            keypair: wg_local_keypair,
        };

        let wg_remote_ip = WIREGUARD_REMOTE_IP.to_string();
        let wg_remote_name = "innisfree_remote".to_string();
        let wg_remote_keypair = WireguardKeypair::new()?;
        let wg_remote_host = WireguardHost {
            name: wg_remote_name.clone(),
            address: wg_remote_ip.clone(),
            endpoint: "".to_string(),
            listenport: WIREGUARD_LISTEN_PORT,
            keypair: wg_remote_keypair,
        };

        let wg_local_device = WireguardDevice {
            name: wg_local_name.clone(),
            interface: wg_local_host.clone(),
            peer: wg_remote_host.clone(),
        };
        let wg_remote_device = WireguardDevice {
            name: wg_remote_name.clone(),
            interface: wg_remote_host.clone(),
            peer: wg_local_host.clone(),
        };

        Ok(WireguardManager {
            wg_local_ip,
            wg_local_name,
            wg_local_host,

            wg_remote_ip,
            wg_remote_name,
            wg_remote_host,

            wg_local_device,
            wg_remote_device,
        })
    }
}

fn generate_wireguard_privkey() -> Result<String, InnisfreeError> {
    // Call out to "wg genkey" and collect output.
    // Ideally we'd generate these values in pure Rust, but
    // calling out to wg as a first draft.
    let privkey_cmd = std::process::Command::new("wg")
        .args(&["genkey"])
        .output()?;
    let privkey: String = str::from_utf8(&privkey_cmd.stdout)
        .unwrap()
        .trim()
        .to_string();
    Ok(privkey)
}

fn derive_wireguard_pubkey(privkey: &str) -> Result<String, InnisfreeError> {
    // Open a pipe to 'wg genkey', to pass in the privkey
    let pubkey_cmd = Command::new("wg")
        .args(&["pubkey"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // Write wg privkey to stdin on pubkey process
    pubkey_cmd
        .stdin
        .unwrap()
        .write_all(privkey.as_bytes())
        .unwrap();

    let mut pubkey = String::new();
    pubkey_cmd
        .stdout
        .unwrap()
        .read_to_string(&mut pubkey)
        .unwrap();

    pubkey = pubkey.trim().to_string();
    Ok(pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_generation() {
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: "foo1".to_string(),
            interface: wg_hosts[0].clone(),
            peer: wg_hosts[1].clone(),
        };
        let wg_config = wg_device.config();
        assert!(wg_config.contains("Interface"));
        assert!(wg_config.contains("PrivateKey = "));

        assert!(!wg_config.contains(&wg_hosts[0].keypair.public));
        assert!(wg_config.contains(&wg_hosts[0].keypair.private));

        assert!(wg_config.contains(&wg_hosts[1].keypair.public));
        assert!(!wg_config.contains(&wg_hosts[1].keypair.private));

        // Slashes '/' will be rendered as hex value &#x2F if formatting is broken
        assert!(!wg_config.contains("&#x2F"));
        assert!(!wg_config.contains(r"&#x2F"));
    }

    // Helper function for reusable structs
    fn _generate_hosts() -> Vec<WireguardHost> {
        let kp1 = WireguardKeypair::new().unwrap();
        let h1 = WireguardHost {
            name: "foo1".to_string(),
            address: "127.0.0.1".to_string(),
            endpoint: "1.1.1.1".to_string(),
            listenport: 80,
            keypair: kp1,
        };
        let kp2 = WireguardKeypair::new().unwrap();
        let h2 = WireguardHost {
            name: "foo2".to_string(),
            address: "127.0.0.1".to_string(),
            endpoint: "".to_string(),
            listenport: 80,
            keypair: kp2,
        };
        let mut wg_hosts: Vec<WireguardHost> = vec![];
        wg_hosts.push(h1);
        wg_hosts.push(h2);
        return wg_hosts;
    }

    #[test]
    fn host_generation() {
        let wg_hosts = _generate_hosts();
        assert_eq!(wg_hosts[0].name, "foo1");
        assert_eq!(wg_hosts[1].name, "foo2");
    }

    #[test]
    fn device_generation() {
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: "foo".to_string(),
            interface: wg_hosts[0].clone(),
            peer: wg_hosts[1].clone(),
        };
        assert_eq!(wg_device.name, "foo");
        assert_eq!(wg_hosts[0].name, "foo1");
    }

    #[test]
    fn host_cloning() {
        let wg_hosts = _generate_hosts();
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
    }

    #[test]
    fn device_cloning() {
        let wg_hosts = _generate_hosts();
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
    }

    #[test]
    fn pubkey_generation() {
        // Use hardcoded privkey value, to compare results with
        // 'wg genkey | wg pubkey'
        let privkey = String::from("yPgz26A4S6RcniNaikFZrc0C0SyCW1moXmDP7AMeimE=");
        let expected_pubkey = "ISRq2SHZQDnSfV0VlmMEP4MbwfExE/iNHzthMQ7eNmY=";
        debug!("Expecting pubkey: {}", expected_pubkey);
        let pubkey = derive_wireguard_pubkey(&privkey).unwrap();
        debug!("Found pubkey: {}", pubkey);
        assert_eq!(pubkey, "ISRq2SHZQDnSfV0VlmMEP4MbwfExE/iNHzthMQ7eNmY=");
    }

    #[test]
    fn key_generation() {
        let kp = WireguardKeypair::new().unwrap();
        assert!(!kp.public.ends_with("\n"));
        assert!(!kp.private.ends_with("\n"));
        // Slashes '/' will be rendered as hex value &#x2F if formatting is broken
        // Confirming they're NOT in the raw key parts, looks like they slipped
        // in during development in the tera template output.
        assert!(!kp.public.contains("&#x2F"));
        assert!(!kp.public.contains(r"&#x2F"));
        assert!(!kp.private.contains("&#x2F"));
        assert!(!kp.private.contains(r"&#x2F"));
    }
}
