//! Utility functions for looking up available
//! IP ranges for establishing the Wireguard interface.

use anyhow::{anyhow, Result};
use ipnet::IpNet;
use std::net::IpAddr;
/// Network subnet range for doling out IP addresses for the Innisfree tunnels.
/// Each instance of innisfree, regardless of the number of [crate::config::ServicePort]s
/// in play, requires a `/30` subnet, that is, two (2) unique IP addresses.
/// We set a `/28` as a the parent range, in case some of those available
/// IPs are already claimed, whether by a different instance of innisfree,
/// or something else entirely.
pub const INNISFREE_SUBNET: &str = "10.50.0.1/28";

/// Checks whether IpAddr exists on local system, whether
/// it is bound to a local device. If not, assumed to be available.
fn address_in_use(ip: IpAddr) -> bool {
    let mut in_use = false;
    for iface in pnet::datalink::interfaces() {
        for i in iface.ips {
            if i.ip() == ip {
                in_use = true;
            }
        }
    }
    in_use
}

/// Returns true if none of the addresses in the subnet
/// are bound on the current system, i.e. all are available.
fn subnet_available(n: IpNet) -> bool {
    let mut is_available = true;
    for h in n.hosts() {
        if address_in_use(h) {
            is_available = false;
        }
    }
    is_available
}

/// Uses the constant INNISFREE_SUBNET `parent_subnet` defines a range in which IP addresses may be claimed.
/// Within that range, an unused /30 will be returned if possible. Otherwise,
/// an error is returned. The /30 setting for child subnets is hardcoded,
/// because the WireguardManager only cares about pairs of 2 addresses, i.e. /30.
pub fn generate_unused_subnet() -> Result<IpNet> {
    let parent_net: IpNet = INNISFREE_SUBNET.parse()?;
    let subnets = parent_net.subnets(30)?.collect::<Vec<IpNet>>();
    for subnet in subnets {
        // Skip initial subnet, which is the entirety of the parent_net, /28.
        // We only consider /30s.
        if subnet.hosts().count() > 2 {
            continue;
        }
        if subnet_available(subnet) {
            return Ok(subnet);
        }
    }
    Err(anyhow!(format!(
        "No available subnets within {}",
        parent_net
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_generation() -> anyhow::Result<()> {
        let n = generate_unused_subnet()?;
        // Ideally we'd test against 10.50.0.1/30, which is the same,
        // but breaks equality assertion.
        let x: ipnet::IpNet = "10.50.0.0/30".parse()?;
        assert_eq!(n, x);
        Ok(())
    }
}
