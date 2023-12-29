//! Stores business logic around creating the "cloud-init.cfg" YAML file,
//! used to customize a server on first boot.
use std::net::IpAddr;

use anyhow::{Context, Result};
extern crate serde;
use serde::{Deserialize, Serialize};

use crate::config::ServicePort;
// TODO the ssh key impl should be provider agnostic
use crate::server::digitalocean::ssh_key::get_all_keys;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;

#[derive(Debug, Serialize, Deserialize)]
/// Representation of a cloudinit YAML file.
/// Support serialization so it can be rendered as a string
/// as part of cloud API calls.
pub struct CloudConfig {
    users: Vec<CloudConfigUser>,
    package_update: bool,
    package_upgrade: bool,
    ssh_keys: std::collections::HashMap<String, String>,
    write_files: Vec<CloudConfigFile>,
    packages: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
/// Represents a ``write_file`` within the `CloudConfig`.
/// See documentation at <https://cloudinit.readthedocs.io/en/latest/topics/modules.html#write-files>.
pub struct CloudConfigFile {
    content: String,
    owner: String,
    path: String,
    permissions: String,
}

#[derive(Debug, Serialize, Deserialize)]
/// Represents a ``user`` within the `CloudConfig`.
/// See documentation at <https://cloudinit.readthedocs.io/en/latest/topics/modules.html#users-and-groups>.
pub struct CloudConfigUser {
    name: String,
    groups: Vec<String>,
    sudo: String,
    shell: String,
    ssh_authorized_keys: Vec<String>,
}

/// Returns a string representation of a cloudinit YAML file.
pub async fn generate_user_data(
    ssh_client_keypair: &SshKeypair,
    ssh_server_keypair: &SshKeypair,
    wg_mgr: &WireguardManager,
    services: &[ServicePort],
) -> Result<String> {
    let user_data = include_str!("../../files/cloudinit.cfg");
    let user_data = user_data.to_string();

    let mut cloud_config = serde_yaml::from_str::<CloudConfig>(&user_data)?;
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
        content: wg_mgr.wg_remote_device.config()?,
        owner: String::from("root:root"),
        permissions: String::from("0644"),
        path: String::from("/tmp/innisfree.conf"),
    };
    cloud_config.write_files.push(wg);

    let nginx = CloudConfigFile {
        content: nginx_streams(services, wg_mgr.wg_local_device.interface.address)?,
        owner: String::from("root:root"),
        permissions: String::from("0644"),
        path: String::from("/etc/nginx/conf.d/stream/innisfree.conf"),
    };
    cloud_config.write_files.push(nginx);

    // Build list of pubkeys to add to cloudinit. There may be no keys
    // returned from the API, e.g. during testing. That's fine,
    // we'll just use the one we generated.
    let mut cloud_config_ssh_keys = vec![ssh_client_keypair.public.to_string()];
    match get_all_keys().await {
        Ok(r) => {
            for k in r {
                cloud_config_ssh_keys.extend(vec![k.public_key.to_owned()]);
            }
        }
        Err(e) => {
            warn!("No SSH pubkeys found via API: {}", e);
        }
    }

    cloud_config.users[0].ssh_authorized_keys = cloud_config_ssh_keys;

    let cc_rendered: String = serde_yaml::to_string(&cloud_config)?;
    let cc_rendered_no_header = &cc_rendered.as_bytes()[4..];
    let cc_rendered = std::str::from_utf8(cc_rendered_no_header)?;
    let mut cc: String = String::from("#cloud-config");
    cc.push('\n');
    cc.push_str(cc_rendered);
    Ok(cc)
}

/// Generates an nginx stream configuration file as a string,
/// for use configuring the remote server's nginx proxy.
// TODO consider using caddy for this. Ideally we'd terminate
// TLS locally, but it'd sure be convenient.
fn nginx_streams(services: &[ServicePort], dest_ip: IpAddr) -> Result<String> {
    let nginx_config = include_str!("../../files/stream.conf.j2");
    let mut context = tera::Context::new();
    context.insert("services", services);
    context.insert("dest_ip", &dest_ip.to_string());
    // Disable autoescaping, since it breaks wg key contents
    tera::Tera::one_off(nginx_config, &context, false).context("Template generation failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wg::{WireguardHost, WireguardKeypair};

    // Helper function for reusable structs
    // This function is copied from src/wg.rs,
    // figure out a way to reuse it safely
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

    #[tokio::test]
    async fn cloudconfig_has_header() -> Result<()> {
        let kp1 = SshKeypair::new("server-test1")?;
        let kp2 = SshKeypair::new("server-test2")?;
        let wg_mgr = WireguardManager::new("foo-test")?;
        let ports = vec![];
        let user_data = generate_user_data(&kp1, &kp2, &wg_mgr, &ports).await?;
        assert!(user_data.ends_with(""));
        assert!(user_data.starts_with("#cloud-config"));
        assert!(user_data.starts_with("#cloud-config\n"));
        Ok(())
    }
}
