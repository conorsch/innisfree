use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::str;

use crate::config::make_config_dir;

extern crate tera;

extern crate serde;
use serde::Serialize;

// Cutting corners here. IP addresses should be customizable,
// and be a valid /30.
const WIREGUARD_LISTEN_PORT: i32 = 51820;
const WIREGUARD_LOCAL_IP: &str = "10.50.0.1";
const WIREGUARD_REMOTE_IP: &str = "10.50.0.2";

#[derive(Debug, Serialize, Clone)]
pub struct WireguardKeypair {
    private: String,
    public: String,
}

impl WireguardKeypair {
    pub fn new() -> WireguardKeypair {
        generate_wireguard_keypair()
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

#[derive(Debug, Serialize)]
pub struct WireguardDevice {
    name: String,
    hosts: Vec<WireguardHost>,
}

impl WireguardDevice {
    pub fn new(name: &str, hosts: Vec<WireguardHost>) -> WireguardDevice {
        WireguardDevice {
            name: name.to_string(),
            hosts: hosts,
        }
    }
    // Returns contents of an INI config file for WG, e.g. 'wg0.conf' in docs.
    pub fn config(&self) -> String {
        let wg_template = include_str!("../files/wg0.conf.j2");
        let mut context = tera::Context::new();
        context.insert("wireguard_name", &self.name);
        context.insert("wireguard_hosts", &self.hosts);
        let result = tera::Tera::one_off(wg_template, &context, true).unwrap();
        return result;
    }

    pub fn write_config(&self) {
        let mut wg_config_path = std::path::PathBuf::from(make_config_dir());
        wg_config_path.push("innisfree.conf");
        let mut f = std::fs::File::create(&wg_config_path).unwrap();
        f.write_all(&self.config().as_bytes()).unwrap();
    }
}

#[derive(Debug)]
pub struct WireguardManager {
    pub wg_local_ip: String,
    wg_local_name: String,
    wg_local_host: WireguardHost,
    pub wg_local_device: WireguardDevice,

    pub wg_remote_ip: String,
    wg_remote_name: String,
    wg_remote_host: WireguardHost,
    pub wg_remote_device: WireguardDevice,

    pub hosts: Vec<WireguardHost>,
}

impl WireguardManager {
    pub fn new() -> WireguardManager {
        let wg_local_ip = WIREGUARD_LOCAL_IP.to_string();
        let wg_local_name = "innisfree_local".to_string();
        let wg_local_host = WireguardHost {
            name: wg_local_name.clone(),
            address: wg_local_ip.clone(),
            endpoint: "".to_string(),
            listenport: 0,
            keypair: WireguardKeypair::new(),
        };

        let wg_remote_ip = WIREGUARD_REMOTE_IP.to_string();
        let wg_remote_name = "innisfree_remote".to_string();
        let wg_remote_host = WireguardHost {
            name: wg_remote_name.clone(),
            address: wg_remote_ip.clone(),
            endpoint: "".to_string(),
            listenport: WIREGUARD_LISTEN_PORT,
            keypair: WireguardKeypair::new(),
        };
        let hosts = vec![wg_local_host.clone(), wg_remote_host.clone()];
        // Intentionally using constructor and direct struct instantiation,
        // to compare visually. Don't have a sense for which is idiomatic yet,
        // although I suspect it's the direct struct. Maybe Clippy knows.
        let wg_local_device = WireguardDevice::new(&wg_local_name.clone(), hosts.clone());
        let wg_remote_device = WireguardDevice {
            name: wg_remote_name.clone(),
            hosts: hosts.clone(),
        };

        WireguardManager {
            wg_local_ip: wg_local_ip,
            wg_local_name: wg_local_name,
            wg_local_host: wg_local_host.clone(),

            wg_remote_ip: wg_remote_ip,
            wg_remote_name: wg_remote_name,
            wg_remote_host: wg_remote_host.clone(),

            wg_local_device: wg_local_device,
            wg_remote_device: wg_remote_device,

            hosts: hosts,
        }
    }
}

fn generate_wireguard_keypair() -> WireguardKeypair {
    // Call out to "wg genkey" and collect output.
    // Ideally we'd generate these values in pure Rust, but
    // calling out to wg as a first draft.
    let privkey_cmd = std::process::Command::new("wg")
        .args(&["genkey"])
        .output()
        .expect("Failed to generate Wireguard private key");

    let privkey: String = str::from_utf8(&privkey_cmd.stdout).unwrap().to_string();

    // Open a pipe to 'wg genkey', to pass in the privkey
    let pubkey_cmd = Command::new("wg")
        .args(&["genkey"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

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

    let kp = WireguardKeypair {
        private: privkey,
        public: pubkey,
    };
    return kp;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_generation() {
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: "foo1".to_string(),
            hosts: wg_hosts,
        };
        let wg_config = wg_device.config();
        assert!(wg_config.contains("Interface"));
        assert!(wg_config.contains("PrivateKey = "));
    }

    // Helper function for reusable structs
    fn _generate_hosts() -> Vec<WireguardHost> {
        let kp1 = WireguardKeypair::new();
        let h1 = WireguardHost {
            name: "foo1".to_string(),
            address: "127.0.0.1".to_string(),
            endpoint: "1.1.1.1".to_string(),
            listenport: 80,
            keypair: kp1,
        };
        let kp2 = WireguardKeypair::new();
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
        assert!(wg_hosts[0].name == "foo1");
        assert!(wg_hosts[1].name == "foo2");
    }

    #[test]
    fn device_generation() {
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: "foo".to_string(),
            hosts: wg_hosts,
        };
        assert!(wg_device.name == "foo");
        assert!(wg_device.hosts[0].name == "foo1");
    }
}
