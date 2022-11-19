extern crate home;

use anyhow::Result;
use serde::Serialize;
use std::convert::TryFrom;
use std::path::PathBuf;

// Describes a request for a port to expose.
// Will be passed around to nginx and wireguard configuration logic
// to build out the tunnel.
#[derive(Debug, Clone, Serialize)]
pub struct ServicePort {
    pub port: i32,
    pub protocol: String,
}

impl ServicePort {
    pub fn from_str_multi(port_spec: &str) -> Result<Vec<ServicePort>> {
        Ok(port_spec
            .split(',')
            .map(|sp| ServicePort::try_from(sp).unwrap())
            .collect())
    }
}

impl TryFrom<&str> for ServicePort {
    type Error = anyhow::Error;

    fn try_from(port_spec: &str) -> Result<Self> {
        let mut sp = ServicePort {
            port: 80,
            protocol: "TCP".to_string(),
        };
        if port_spec.contains('/') {
            let port_spec_parts: Vec<&str> = port_spec.split('/').collect();
            let port: i32 = port_spec_parts[0].parse::<i32>()?;
            let protocol: String = port_spec_parts[1].to_string();
            sp.port = port;
            sp.protocol = protocol;
        } else {
            let port: i32 = port_spec.parse::<i32>()?;
            let protocol: String = "TCP".to_string();
            sp.port = port;
            sp.protocol = protocol;
        }
        Ok(sp)
    }
}

pub fn make_config_dir(service_name: &str) -> Result<PathBuf> {
    let mut config_dir = home::home_dir().unwrap();
    config_dir.push(".config");
    config_dir.push("innisfree");
    config_dir.push(service_name);
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

pub fn clean_config_dir(service_name: &str) -> Result<()> {
    let config_dir = make_config_dir(service_name)?;
    for f in std::fs::read_dir(&config_dir)? {
        let f = f?;
        std::fs::remove_file(f.path())?;
    }
    std::fs::remove_dir(config_dir)?;
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
        let s = ServicePort {
            port: 80,
            protocol: "TCP".to_string(),
        };
        assert!(s.port == 80);
        assert!(s.protocol == "TCP");
    }

    #[test]
    fn web_ports_parse_ok() {
        let port_spec = "80/TCP,443/TCP";
        let services = ServicePort::from_str_multi(port_spec).unwrap();
        assert!(services.len() == 2);
        let s1 = &services[0];
        assert!(s1.port == 80);
        assert!(s1.protocol == "TCP");

        let s2 = &services[1];
        assert!(s2.port == 443);
        assert!(s2.protocol == "TCP");
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
