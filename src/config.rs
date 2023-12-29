//! Storage logic, to persist configuration of remote tunnels locally.
//! Includes methods for creating and destroying configuration directories.

use anyhow::Result;

use serde::Serialize;
use std::convert::TryFrom;
use std::path::PathBuf;

// Define public exports
const DEFAULT_PORT: i32 = 80;
const DEFAULT_LOCAL_PORT: i32 = 80;

/// Describes a socket expectation for a given service.
/// The port will be reused to listen locally and forward remotely.
// Will be passed around to nginx and wireguard configuration logic
// to build out the tunnel.
#[derive(Debug, Clone, Serialize)]
pub struct ServicePort {
    /// Port number for the public service.
    pub port: i32,
    /// Port number for the local service, to which traffic is forwarded.
    pub local_port: i32,
    /// Protocol, one of TCP or UDP.
    pub protocol: String,
}

impl ServicePort {
    /// Parse a comma-separated string of ServicePort specs,
    /// e.g. `8080/TCP,4444/UDP`.
    pub fn from_str_multi(port_spec: &str) -> Result<Vec<ServicePort>> {
        Ok(port_spec
            .split(',')
            .flat_map(ServicePort::try_from)
            .collect())
    }
}

impl Default for ServicePort {
    fn default() -> Self {
        ServicePort {
            port: DEFAULT_PORT,
            local_port: DEFAULT_LOCAL_PORT,
            protocol: "TCP".to_string(),
        }
    }
}

/// We implement `TryFrom<&str>` so we can parse CLI args.
impl TryFrom<&str> for ServicePort {
    type Error = anyhow::Error;

    /// Handles str specs such as:
    ///
    ///   * `80/TCP`
    ///   * `80`
    ///   * `80:80`
    ///   * `88888:9999`
    ///
    /// In the format `8888:9999`, `8888` remote port on the public ingress,
    /// and `9999` is the local port of the service to forward traffic to.
    fn try_from(port_spec: &str) -> Result<Self> {
        let mut sp = ServicePort::default();
        // Handle optional protocol spec
        let port_and_proto_spec: Vec<String> =
            port_spec.split('/').map(|x| x.to_string()).collect();
        sp.protocol = match port_and_proto_spec.get(1) {
            Some(p) => p.to_string(),
            None => String::from("TCP"),
        };

        // Handle port spec, with optional local/remote distinction
        let port_spec = &port_and_proto_spec[0];
        let port_spec_parts: Vec<String> = port_spec.split(':').map(|x| x.to_string()).collect();
        sp.port = port_spec_parts[0].parse()?;
        sp.local_port = match port_spec_parts.get(1) {
            Some(p) => p.parse()?,
            None => sp.port,
        };
        Ok(sp)
    }
}

/// Create local config dir, e.g. ~/.config/innisfree/,
/// for storing state of active tunnels.
pub fn make_config_dir(service_name: &str) -> Result<PathBuf> {
    let config_dir = home::home_dir()
        .ok_or(anyhow::anyhow!("could not find home directory"))?
        .join(".config")
        .join("innisfree")
        .join(service_name);
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

/// Remove config dir and all contents.
/// Will render active tunnels unconfigurable,
/// and subject to manual cleanup.
pub fn clean_config_dir(service_name: &str) -> Result<()> {
    let config_dir = make_config_dir(service_name)?;
    debug!("Removing config dir: {}", config_dir.display());
    std::fs::remove_dir_all(config_dir)?;
    Ok(())
}

/// Provides a human-readable name for the service.
/// Adds a prefix "innisfree-" if it does not exist.
pub fn clean_name(name: &str) -> String {
    let mut orig = String::from(name);
    if orig == "innisfree" {
        return orig;
    }
    orig = orig.replace("-innisfree", "");
    orig = orig.replace("innisfree-", "");
    let mut result = String::from("innisfree-");
    result.push_str(&orig);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_port_manual_creation() {
        let s = ServicePort::default();
        assert!(s.port == 80);
        assert!(s.protocol == "TCP");
    }

    #[test]
    fn parse_web_ports() -> Result<()> {
        let port_spec = "80/TCP,443/TCP";
        let services = ServicePort::from_str_multi(port_spec)?;
        assert!(services.len() == 2);
        let s1 = &services[0];
        assert!(s1.port == 80);
        assert!(s1.protocol == "TCP");

        let s2 = &services[1];
        assert!(s2.port == 443);
        assert!(s2.protocol == "TCP");
        Ok(())
    }

    #[test]
    fn parse_different_ports() -> Result<()> {
        let port_spec = "80:30080/TCP";
        let s = ServicePort::try_from(port_spec)?;
        assert!(s.port == 80);
        assert!(s.local_port == 30080);
        assert!(s.protocol == "TCP");
        Ok(())
    }
    #[test]
    fn parse_different_ports_multi() -> Result<()> {
        let port_spec = "80:30080,443:30443";
        let services = ServicePort::from_str_multi(port_spec)?;
        assert!(services.len() == 2);
        let s1 = &services[0];
        assert!(s1.port == 80);
        assert!(s1.local_port == 30080);
        assert!(s1.protocol == "TCP");

        let s2 = &services[1];
        assert!(s2.port == 443);
        assert!(s2.local_port == 30443);
        assert!(s2.protocol == "TCP");
        Ok(())
    }
    #[test]
    fn clean_service_name() {
        let s_simple = "foo";
        let r_simple = clean_name(s_simple);
        assert!(r_simple == *"innisfree-foo");

        let s_complex = "foo-innisfree";
        let r_complex = clean_name(s_complex);
        assert!(r_complex == *"innisfree-foo");

        let s_default = "innisfree";
        let r_default = clean_name(s_default);
        assert!(r_default == *"innisfree");
    }
}
