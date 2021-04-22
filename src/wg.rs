use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::str;

#[derive(Debug)]
pub struct WireguardKeypair {
    private: String,
    public: String,
}

impl WireguardKeypair {
    pub fn new() -> WireguardKeypair {
        generate_wireguard_keypair()
    }
}

#[derive(Debug)]
pub struct WireguardHost {
    name: String,
    address: String,
    endpoint: String,
    listenport: i32,
    keypair: WireguardKeypair,
}

#[derive(Debug)]
pub struct WireguardDevice {
    name: String,
    hosts: Vec<WireguardHost>,
}

impl WireguardDevice {
    // Returns contents of an INI config file for WG, e.g. 'wg0.conf' in docs.
    pub fn config() -> String {
        let mut wg_template = include_str!("../files/wg0.conf.j2");
        return wg_template.to_string();
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
    fn config_generation() {}


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
