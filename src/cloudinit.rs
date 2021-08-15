// Stores business logic around creating the "cloud-init.cfg" YAML file,
// used to customize a server on first boot.
extern crate serde;
use serde::{Deserialize, Serialize};

use crate::config::{InnisfreeError, ServicePort};
use crate::ssh::SshKeypair;
use crate::wg::{WireguardDevice, WIREGUARD_LOCAL_IP};

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudConfig {
    users: Vec<CloudConfigUser>,
    package_update: bool,
    package_upgrade: bool,
    ssh_keys: std::collections::HashMap<String, String>,
    write_files: Vec<CloudConfigFile>,
    packages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudConfigFile {
    content: String,
    owner: String,
    path: String,
    permissions: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudConfigUser {
    name: String,
    groups: Vec<String>,
    sudo: String,
    shell: String,
    ssh_authorized_keys: Vec<String>,
}

pub fn generate_user_data(
    ssh_client_keypair: &SshKeypair,
    ssh_server_keypair: &SshKeypair,
    wg_device: &WireguardDevice,
    services: &[ServicePort],
) -> Result<String, InnisfreeError> {
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
        // Use the template without firewall rules
        content: wg_device.config(),
        owner: String::from("root:root"),
        permissions: String::from("0644"),
        path: String::from("/tmp/innisfree.conf"),
    };
    cloud_config.write_files.push(wg);

    let nginx = CloudConfigFile {
        content: nginx_streams(services)?,
        owner: String::from("root:root"),
        permissions: String::from("0644"),
        path: String::from("/etc/nginx/conf.d/stream/innisfree.conf"),
    };
    cloud_config.write_files.push(nginx);

    cloud_config.users[0].ssh_authorized_keys = vec![ssh_client_keypair.public.to_string()];

    let cc_rendered: String = serde_yaml::to_string(&cloud_config).unwrap();
    let cc_rendered_no_header = &cc_rendered.as_bytes()[4..];
    let cc_rendered = std::str::from_utf8(cc_rendered_no_header).unwrap();
    let mut cc: String = String::from("#cloud-config");
    cc.push('\n');
    cc.push_str(cc_rendered);
    Ok(cc)
}

fn nginx_streams(services: &[ServicePort]) -> Result<String, InnisfreeError> {
    let nginx_config = include_str!("../files/stream.conf.j2");
    let mut context = tera::Context::new();
    context.insert("services", services);
    context.insert("dest_ip", &WIREGUARD_LOCAL_IP.to_string());
    // Disable autoescaping, since it breaks wg key contents
    let result = tera::Tera::one_off(nginx_config, &context, false);
    match result {
        Ok(r) => Ok(r),
        Err(e) => Err(InnisfreeError::Template { source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wg::{WireguardDevice, WireguardHost, WireguardKeypair};

    // Helper function for reusable structs
    // This function is copied from src/wg.rs,
    // figure out a way to reuse it safely
    fn _generate_hosts() -> Vec<WireguardHost> {
        let kp1 = WireguardKeypair::new().unwrap();
        let h1 = WireguardHost {
            name: "foo1".to_string(),
            address: "127.0.0.1".parse().unwrap(),
            endpoint: Some("1.1.1.1".parse().unwrap()),
            listenport: 80,
            keypair: kp1,
        };
        let kp2 = WireguardKeypair::new().unwrap();
        let h2 = WireguardHost {
            name: "foo2".to_string(),
            address: "127.0.0.1".parse().unwrap(),
            endpoint: None,
            listenport: 80,
            keypair: kp2,
        };
        let wg_hosts: Vec<WireguardHost> = vec![h1, h2];
        wg_hosts
    }

    #[test]
    fn cloudconfig_has_header() {
        let kp1 = SshKeypair::new("server-test1").unwrap();
        let kp2 = SshKeypair::new("server-test2").unwrap();
        let wg_hosts = _generate_hosts();
        let wg_device = WireguardDevice {
            name: String::from("foo1"),
            interface: wg_hosts[1].clone(),
            peer: wg_hosts[0].clone(),
        };
        let ports = vec![];
        let user_data = generate_user_data(&kp1, &kp2, &wg_device, &ports).unwrap();
        assert!(user_data.ends_with(""));
        assert!(user_data.starts_with("#cloud-config"));
        assert!(user_data.starts_with("#cloud-config\n"));
    }
}
