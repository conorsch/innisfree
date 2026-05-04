//! Storage logic, to persist configuration of remote tunnels locally.
//! Includes methods for creating and destroying configuration directories.

use anyhow::{anyhow, Result};

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::path::PathBuf;
use std::str::FromStr;

// Define public exports
const DEFAULT_PORT: i32 = 80;
const DEFAULT_LOCAL_PORT: i32 = 80;

/// Transport protocol for a forwarded service. The serialized form is
/// `"TCP"` / `"UDP"` so the existing Tera templates that compare
/// `s.protocol == "TCP"` continue to work without edits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Protocol {
    /// Transmission Control Protocol.
    #[default]
    Tcp,
    /// User Datagram Protocol.
    Udp,
}

impl FromStr for Protocol {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "TCP" => Ok(Protocol::Tcp),
            "UDP" => Ok(Protocol::Udp),
            other => Err(anyhow!("unknown protocol {other:?}, expected TCP or UDP")),
        }
    }
}

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
    pub protocol: Protocol,
}

impl ServicePort {
    /// Parse a comma-separated string of ServicePort specs,
    /// e.g. `8080/TCP,4444/UDP`.
    pub fn from_str_multi(port_spec: &str) -> Result<Vec<ServicePort>> {
        port_spec.split(',').map(ServicePort::try_from).collect()
    }
}

impl Default for ServicePort {
    fn default() -> Self {
        ServicePort {
            port: DEFAULT_PORT,
            local_port: DEFAULT_LOCAL_PORT,
            protocol: Protocol::default(),
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
        let (port_part, protocol) = match port_spec.split_once('/') {
            Some((ports, proto)) => (ports, proto.parse()?),
            None => (port_spec, Protocol::default()),
        };
        let (port, local_port) = match port_part.split_once(':') {
            Some((remote, local)) => (remote.parse()?, local.parse()?),
            None => {
                let p: i32 = port_part.parse()?;
                (p, p)
            }
        };
        Ok(ServicePort {
            port,
            local_port,
            protocol,
        })
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
    tracing::debug!("Removing config dir: {}", config_dir.display());
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
        assert_eq!(s.port, 80);
        assert_eq!(s.protocol, Protocol::Tcp);
    }

    #[test]
    fn parse_web_ports() -> Result<()> {
        let port_spec = "80/TCP,443/TCP";
        let services = ServicePort::from_str_multi(port_spec)?;
        assert_eq!(services.len(), 2);
        let s1 = &services[0];
        assert_eq!(s1.port, 80);
        assert_eq!(s1.protocol, Protocol::Tcp);

        let s2 = &services[1];
        assert_eq!(s2.port, 443);
        assert_eq!(s2.protocol, Protocol::Tcp);
        Ok(())
    }

    #[test]
    fn parse_different_ports() -> Result<()> {
        let port_spec = "80:30080/TCP";
        let s = ServicePort::try_from(port_spec)?;
        assert_eq!(s.port, 80);
        assert_eq!(s.local_port, 30080);
        assert_eq!(s.protocol, Protocol::Tcp);
        Ok(())
    }
    #[test]
    fn parse_different_ports_multi() -> Result<()> {
        let port_spec = "80:30080,443:30443";
        let services = ServicePort::from_str_multi(port_spec)?;
        assert_eq!(services.len(), 2);
        let s1 = &services[0];
        assert_eq!(s1.port, 80);
        assert_eq!(s1.local_port, 30080);
        assert_eq!(s1.protocol, Protocol::Tcp);

        let s2 = &services[1];
        assert_eq!(s2.port, 443);
        assert_eq!(s2.local_port, 30443);
        assert_eq!(s2.protocol, Protocol::Tcp);
        Ok(())
    }

    #[test]
    fn from_str_multi_propagates_parse_errors() {
        // Previous `flat_map` shape silently dropped the bad spec; assert we
        // now surface the failure instead of returning a partial vec.
        let err = ServicePort::from_str_multi("80/TCP,not-a-port").unwrap_err();
        assert!(
            err.to_string().contains("invalid digit") || err.to_string().contains("not-a-port"),
            "expected a parse error, got: {err}"
        );
    }

    #[test]
    fn parse_protocol_is_case_insensitive() -> Result<()> {
        assert_eq!("tcp".parse::<Protocol>()?, Protocol::Tcp);
        assert_eq!("Udp".parse::<Protocol>()?, Protocol::Udp);
        assert!("sctp".parse::<Protocol>().is_err());
        Ok(())
    }

    #[test]
    fn protocol_serializes_as_uppercase() -> Result<()> {
        // Tera templates compare against the literal strings "TCP" / "UDP";
        // lock that wire format in so a future serde rename can't quietly
        // break the rendered nginx and wg configs.
        let s = ServicePort {
            port: 80,
            local_port: 80,
            protocol: Protocol::Udp,
        };
        let j = serde_json::to_string(&s)?;
        assert!(j.contains("\"protocol\":\"UDP\""), "got: {j}");
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
