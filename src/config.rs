extern crate custom_error;
extern crate home;
use custom_error::custom_error;

use serde::Serialize;

// Describes a request for a port to expose.
// Will be passed around to nginx and wireguard configuration logic
// to build out the tunnel.
#[derive(Debug, Clone, Serialize)]
pub struct ServicePort {
    pub port: i32,
    pub protocol: String,
}

impl ServicePort {
    pub fn from_str(port_spec: &str) -> ServicePort {
        let mut sp = ServicePort {
            port: 80,
            protocol: "TCP".to_string(),
        };
        if port_spec.contains('/') {
            let port_spec_parts: Vec<&str> = port_spec.split('/').collect();
            let port: i32 = port_spec_parts[0].parse::<i32>().unwrap();
            let protocol: String = port_spec_parts[1].to_string();
            sp.port = port;
            sp.protocol = protocol;
        } else {
            let port: i32 = port_spec.parse::<i32>().unwrap();
            let protocol: String = "TCP".to_string();
            sp.port = port;
            sp.protocol = protocol;
        }
        sp
    }

    pub fn from_str_multi(port_spec: &str) -> Vec<ServicePort> {
        let raw_ports: Vec<&str> = port_spec.split(',').collect();
        let mut results: Vec<ServicePort> = vec![];
        for raw_port in raw_ports {
            let sp = ServicePort::from_str(&raw_port);
            results.push(sp);
        }
        results
    }
}

pub fn make_config_dir() -> String {
    let mut config_dir = home::home_dir().unwrap();
    config_dir.push(".config");
    config_dir.push("innisfree");
    std::fs::create_dir_all(&config_dir).unwrap();
    config_dir.to_str().unwrap().to_string()
}

pub fn clean_config_dir() {
    let config_dir = make_config_dir();
    for f in std::fs::read_dir(config_dir).unwrap() {
        let f = f.unwrap();
        std::fs::remove_file(f.path()).unwrap();
    }
}

/// Provides a human-readable name for the service.
/// Adds a prefix "innisfree-" if it does not exist.
pub fn clean_name(name: &str) -> String {
    let mut orig = String::from(name);
    orig = orig.replace("-innisfree", "");
    orig = orig.replace("innisfree-", "");
    let mut result = String::from("innisfree-");
    result.push_str(&orig);
    result
}

// Using custom_error mostly for read/write errors
// Note the use of braces rather than parentheses.
custom_error! {pub InnisfreeError
    Io{source: std::io::Error} = "input/output error",
    // CommandFailure{source: std::process::ExitStatus} = "command failed",
    SshCommandFailure = "SSH command failed",
    ServerNotFound = "Server does not exist",
    CommandFailure{msg: String} = "Local command failed: {}",
    NetworkError{source: reqwest::Error} = "Network error, check connection",
    PlatformError = "Platform error, only Linux is supported",
    Template{source: tera::Error} = "Template generation failed",
    Unknown = "unknown error",
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
        let services = ServicePort::from_str_multi(&port_spec);
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
        assert!(r_simple == "innisfree-foo".to_string());

        let s_complex = "foo-innisfree";
        let r_complex = clean_name(s_complex);
        assert!(r_complex == "innisfree-foo".to_string());
    }
}
