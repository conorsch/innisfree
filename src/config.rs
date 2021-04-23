use std::path::Path;
extern crate home;

#[derive(Debug, Clone)]
pub struct ServicePort {
    port: i32,
    protocol: String,
}

impl ServicePort {
    pub fn from_str(port_spec: &str) -> ServicePort {
        let mut sp = ServicePort {
            port: 80,
            protocol: "TCP".to_string(),
        };
        if port_spec.contains("/") {
            let port_spec_parts: Vec<&str> = port_spec.split("/").collect();
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
        return sp;
    }

    pub fn from_str_multi(port_spec: &str) -> Vec<ServicePort> {
        let raw_ports: Vec<&str> = port_spec.split(",").collect();
        let mut results: Vec<ServicePort> = vec![];
        for raw_port in raw_ports {
            let sp = ServicePort::from_str(&raw_port);
            results.push(sp);
        }
        return results;
    }
}

pub fn make_config_dir() -> String {
    let mut config_dir = home::home_dir().unwrap();
    config_dir.push(".config");
    config_dir.push("innisfree");
    std::fs::create_dir_all(&config_dir).unwrap();
    return config_dir.to_str().unwrap().to_string();
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
}
