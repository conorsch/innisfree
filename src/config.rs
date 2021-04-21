struct ServicePort {
    port: i32,
    protocol: String,
}


pub fn parse_ports(port_spec: &str) -> Vec<ServicePort> {

}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_port_manual_creation() {
        let s: ServicePort = { "port": 80, "protocol": "TCP" };
        assert!(s.port == 80);
        assert!(s.protocol == "TCP");
    }

    #[test]
    fn web_ports_parse_ok() {
        let port_spec = "80/TCP,443/TCP";
        let services = parse_ports(&port_spec);
        assert!(len(services) == 2);
        let s1 = services[0];
        assert!(s1.port == 80);
        assert!(s1.protocol == "TCP");

        let s2 = services[1];
        assert!(s2.port == 443);
        assert!(s2.protocol == "TCP");
    }
}

