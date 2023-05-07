//! Storage logic, to persist configuration of remote tunnels locally.
//! Includes methods for creating and destroying configuration directories.

use anyhow::Result;

use serde::Serialize;
use std::convert::TryFrom;
use std::path::PathBuf;

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
            .map(|s| ServicePort::try_from(s).unwrap())
            .collect())
    }
}

impl Default for ServicePort {
    fn default() -> Self {
        ServicePort {
            port: 80,
            local_port: 80,
            protocol: "TCP".to_string(),
        }
    }
}

/// We implement `TryFrom<&str>` so we can parse CLI args.
impl TryFrom<&str> for ServicePort {
    type Error = anyhow::Error;

    fn try_from(port_spec: &str) -> Result<Self> {
        let mut sp = ServicePort::default();
        if port_spec.contains('/') {
            let port_spec_parts: Vec<&str> = port_spec.split('/').collect();
            sp.protocol = port_spec_parts[1].to_string();
            sp = parse_port_spec(port_spec_parts[0])?;
        } else {
            sp = parse_port_spec(port_spec)?;
        }

        Ok(sp)
    }
}

/// Handles str specs such as:
///
///   * "80/TCP"
///   * "80"
///   * "80:80"
///
fn parse_port_spec(port_spec: &str) -> Result<ServicePort> {
    let mut sp = ServicePort::default();
    if port_spec.contains(':') {
        let port_spec_parts: Vec<&str> = port_spec.split(':').collect();
        sp.port = port_spec_parts[0].parse()?;
        sp.local_port = port_spec_parts[1].parse()?;
    } else {
        sp.port = port_spec.parse()?;
        sp.local_port = port_spec.parse()?;
    }
    Ok(sp)
}

/// Create local config dir, e.g. ~/.config/innisfree/,
/// for storing state of active tunnels.
pub fn make_config_dir(service_name: &str) -> Result<PathBuf> {
    let mut config_dir = home::home_dir().unwrap();
    config_dir.push(".config");
    config_dir.push("innisfree");
    config_dir.push(service_name);
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
