//! Stores business logic around creating the "cloud-init.cfg" YAML file,
//! used to customize a server on first boot.
use std::net::IpAddr;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::ServicePort;

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
/// Represents a `write_file` within the [`CloudConfig`].
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

impl CloudConfig {
    /// Create a cloud-init YAML file for a given tunnel config,
    /// based on services. The resulting config authorizes only
    /// `ssh_client_keypair` for login; callers may add more authorized
    /// keys via [`CloudConfig::authorize_ssh_keys`].
    pub fn new(
        ssh_client_keypair: &SshKeypair,
        ssh_server_keypair: &SshKeypair,
        wg_mgr: &WireguardManager,
        services: &[ServicePort],
    ) -> Result<Self> {
        let cc_template = include_str!("../../files/cloudinit.cfg");
        let mut cc = serde_yaml::from_str::<CloudConfig>(cc_template)?;
        // TODO: add impl to SshKeypair for rendering this format?
        cc.ssh_keys.insert(
            "ed25520_public".to_string(),
            ssh_server_keypair.public.to_string(),
        );
        cc.ssh_keys.insert(
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
        cc.write_files.push(wg);
        let nginx = CloudConfigFile {
            content: nginx_streams(services, wg_mgr.wg_local_device.interface.address)?,
            owner: String::from("root:root"),
            permissions: String::from("0644"),
            path: String::from("/etc/nginx/conf.d/stream/innisfree.conf"),
        };
        cc.write_files.push(nginx);

        cc.users[0].ssh_authorized_keys = vec![ssh_client_keypair.public.to_string()];

        Ok(cc)
    }

    /// Append additional public keys to the primary user's
    /// `authorized_keys`, e.g. operator keys fetched from a cloud
    /// provider's account.
    pub fn authorize_ssh_keys<I, S>(&mut self, public_keys: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.users[0]
            .ssh_authorized_keys
            .extend(public_keys.into_iter().map(Into::into));
    }
}

/// We implement a fallible conversion of [CloudConfig] to [String]
/// so that `user_data` can be provided to cloud APIs.
impl TryInto<String> for CloudConfig {
    type Error = anyhow::Error;

    fn try_into(self) -> anyhow::Result<String> {
        Ok(format!("#cloud-config\n{}", serde_yaml::to_string(&self)?))
    }
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

    #[test]
    fn cloudconfig_has_header() -> Result<()> {
        let kp1 = SshKeypair::new("server-test1")?;
        let kp2 = SshKeypair::new("server-test2")?;
        let wg_mgr = WireguardManager::new("foo-test")?;
        let ports = vec![];
        let cc = CloudConfig::new(&kp1, &kp2, &wg_mgr, &ports)?;
        let user_data: String = cc.try_into()?;
        assert!(user_data.starts_with("#cloud-config\n"));
        Ok(())
    }
}
