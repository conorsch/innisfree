// extern crate digitalocean;
// use digitalocean::prelude::*;
use std::collections::HashMap;
use std::env;
use std::thread;
use std::time;

// Web API request imports, see
// https://rust-lang-nursery.github.io/rust-cookbook/web/clients/apis.html
extern crate reqwest;

extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{make_config_dir, ServicePort};
use crate::ssh::SshKeypair;
use crate::wg::WireguardDevice;

const DO_REGION: &str = "sfo2";
const DO_SIZE: &str = "s-1vcpu-1gb";
const DO_IMAGE: &str = "ubuntu-20-04-x64";
// const DO_NAME: &str = "innisfree";
const DO_NAME: &str = "jawn";
const DO_API_BASE_URL: &str = "https://api.digitalocean.com/v2/droplets";

// Representation of a DigitalOcean Droplet, i.e. cloud VM.
#[derive(Debug, Deserialize)]
struct Droplet {
    id: u32,
    name: String,
    // Field are attributes, will need to retrieve them as foo.slug
    // region: String,
    // size: String,
    // image: String,
    status: String,
    raw_json: Option<String>,
    networks: HashMap<String, Vec<HashMap<String, String>>>,
}

impl Droplet {
    fn new(user_data: &str) -> Droplet {
        debug!("Creating new DigitalOcean Droplet");
        // Build JSON request body, for sending to DigitalOcean API
        let droplet_body = json!({
            "image": DO_IMAGE,
            "name": DO_NAME,
            "region": DO_REGION,
            "size": DO_SIZE,
            "user_data": user_data,
        });

        // The API logic could be abstracted further, in a DigitalOcean Manager.
        // Right now we only create Droplet resources, but an API Firewall would be nice.
        let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
        let request_url = DO_API_BASE_URL;
        let client = reqwest::blocking::Client::new();

        let response = client
            .post(request_url)
            .json(&droplet_body)
            .bearer_auth(api_key)
            .send()
            .unwrap();

        let j: serde_json::Value = response.json().unwrap();
        let d: String = j["droplet"].to_string();
        let droplet: Droplet = serde_json::from_str(&d).unwrap();
        debug!("Server created, waiting for networking");
        droplet.wait_for_boot()
    }

    fn wait_for_boot(&self) -> Droplet {
        // The JSON response for droplet creation won't include info like
        // public IPv4 address, because that hasn't been assigned yet. The 'status'
        // field will show as "new", so wait until it's "active", then network info
        // will be populated. Might be a good use of enums here.
        loop {
            thread::sleep(time::Duration::from_secs(10));
            let droplet: Droplet = get_droplet(&self);
            if droplet.status == "active" {
                return droplet;
            } else {
                info!("Server still booting, waiting...");
            }
        }
    }

    // IPv4 lookup can fail, should return Result to force handling.
    pub fn ipv4_address(&self) -> String {
        let mut ip: String = "".to_string();
        for v4_network in &self.networks["v4"] {
            if v4_network["type"] == "public" {
                ip = v4_network["ip_address"].clone();
                break;
            }
        }
        ip
    }
}

#[derive(Debug)]
pub struct InnisfreeServer {
    pub services: Vec<ServicePort>,
    pub ssh_client_keypair: SshKeypair,
    pub ssh_server_keypair: SshKeypair,
    wg_device: WireguardDevice,
    droplet: Droplet,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudConfig {
    users: Vec<CloudConfigUser>,
    package_update: bool,
    package_upgrade: bool,
    ssh_keys: std::collections::HashMap<String, String>,
    write_files: Vec<CloudConfigFile>,
    packages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudConfigFile {
    content: String,
    owner: String,
    path: String,
    permissions: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudConfigUser {
    name: String,
    groups: Vec<String>,
    sudo: String,
    shell: String,
    ssh_authorized_keys: Vec<String>,
}

impl InnisfreeServer {
    pub fn new(services: Vec<ServicePort>, wg_device: WireguardDevice) -> InnisfreeServer {
        // Initialize variables outside struct, so we'll need to pass them around
        let ssh_client_keypair = SshKeypair::new("client");
        let ssh_server_keypair = SshKeypair::new("server");
        let user_data = generate_user_data(&ssh_client_keypair, &ssh_server_keypair, &wg_device);
        let droplet = Droplet::new(&user_data);
        InnisfreeServer {
            services,
            ssh_client_keypair,
            ssh_server_keypair,
            wg_device,
            droplet,
            name: "innisfree".to_string(),
        }
    }
    pub fn ipv4_address(&self) -> String {
        let droplet = &self.droplet;
        droplet.ipv4_address()
    }
    pub fn write_user_data(&self) {
        // Write full config locally for debugging;
        let user_data = generate_user_data(
            &self.ssh_client_keypair,
            &self.ssh_server_keypair,
            &self.wg_device,
        );
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("cloudinit.cfg");
        std::fs::write(&fpath.to_str().unwrap(), &user_data).expect("Failed to create cloud-init");
    }
}

pub fn generate_user_data(
    ssh_client_keypair: &SshKeypair,
    ssh_server_keypair: &SshKeypair,
    wg_device: &WireguardDevice,
) -> String {
    let user_data = include_str!("../files/cloudinit.cfg");
    let user_data = user_data.to_string();

    let mut cloud_config = serde_yaml::from_str::<CloudConfig>(&user_data).unwrap();
    cloud_config.ssh_keys.insert(
        "ed25519_public".to_string(),
        ssh_server_keypair.public.to_string(),
    );
    cloud_config.ssh_keys.insert(
        "ed25519_private".to_string(),
        ssh_server_keypair.private.to_string(),
    );

    let wg = CloudConfigFile {
        content: wg_device.config(),
        owner: String::from("root:root"),
        permissions: String::from("0644"),
        path: String::from("/tmp/innisfree.conf"),
    };
    cloud_config.write_files.push(wg);

    // For debugging, add another pubkey
    // $ doctl compute ssh-key list -o json | jq -r .[].public_key
    // ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIlLuXP4H+Jrj7wiuaP18nam634kKSNVHJ0SisdFxv3v
    let tmp_pubkey =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIIlLuXP4H+Jrj7wiuaP18nam634kKSNVHJ0SisdFxv3v"
            .to_owned();
    cloud_config.users[0].ssh_authorized_keys =
        vec![ssh_client_keypair.public.to_string(), tmp_pubkey];

    let cc_rendered: String = serde_yaml::to_string(&cloud_config).unwrap();
    let cc_rendered_no_header = &cc_rendered.as_bytes()[4..];
    let cc_rendered = std::str::from_utf8(&cc_rendered_no_header).unwrap();
    let mut cc: String = String::from("#cloud-config");
    cc.push('\n');
    cc.push_str(&cc_rendered);
    cc
}

// The 'networks' field in the API response will be a nested object,
// so let's stub that out so a Droplet can have a .networks field.
#[derive(Debug, Deserialize)]
struct Network {
    ip_address: String,
    netmask: String,
    /// *Note:* Since `type` is a keyword in Rust `kind` is used instead.
    #[serde(rename = "type")]
    kind: String,
    // Gateway won't exist on private, could be an Option<String>
    // gateway: String,
}

#[derive(Debug, Deserialize)]
struct DropletResponse {
    droplet: Droplet,
}

fn get_droplet(droplet: &Droplet) -> Droplet {
    let api_key = env::var("DIGITALOCEAN_API_TOKEN").expect("DIGITALOCEAN_API_TOKEN not set.");
    let request_url = DO_API_BASE_URL.to_owned() + "/" + &droplet.id.to_string();

    let client = reqwest::blocking::Client::new();
    let response = client.get(request_url).bearer_auth(api_key).send().unwrap();

    let j: serde_json::Value = response.json().unwrap();
    let d: String = j["droplet"].to_string();
    serde_json::from_str(&d).unwrap()
}

fn get_mock_droplet_json() -> String {
    let droplet_json = include_str!("../files/droplet.json");
    droplet_json.to_string()
}

#[allow(dead_code)]
fn _create_droplet() -> Droplet {
    let droplet_json = get_mock_droplet_json();
    serde_json::from_str(&droplet_json).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wg::{WireguardDevice, WireguardHost, WireguardKeypair};

    // Helper function for reusable structs
    // This function is copied from src/wg.rs,
    // figure out a way to reuse it safely
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
    fn cloudconfig_has_header() {
        let kp1 = SshKeypair::new("server-test1");
        let kp2 = SshKeypair::new("server-test2");
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: String::from("foo1"),
            interface: wg_hosts[1].clone(),
            peer: wg_hosts[0].clone(),
        };
        let user_data = generate_user_data(&kp1, &kp2, &wg_device);
        assert!(user_data.ends_with(""));
        assert!(user_data.starts_with("#cloud-config"));
        assert!(user_data.starts_with("#cloud-config\n"));
    }
}
