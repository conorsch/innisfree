//! Utility functions for looking up available
//! IP ranges for establishing the Wireguard interface.

use anyhow::{anyhow, Context, Result};
use ipnet::IpNet;
use std::collections::HashSet;
use std::net::IpAddr;

/// Network subnet range for doling out IP addresses for the Innisfree tunnels.
/// Each instance of innisfree, regardless of the number of [crate::config::ServicePort]s
/// in play, requires a `/30` subnet, that is, two (2) unique IP addresses.
/// We set a `/28` as a the parent range, in case some of those available
/// IPs are already claimed, whether by a different instance of innisfree,
/// or something else entirely.
pub const INNISFREE_SUBNET: &str = "10.50.0.1/28";

/// Snapshot of every IP currently bound to a local interface. Calls
/// `getifaddrs(3)` once via `if-addrs`, which is much cheaper than the
/// previous "scan interfaces for each candidate IP" loop.
fn local_addresses() -> Result<HashSet<IpAddr>> {
    Ok(if_addrs::get_if_addrs()
        .context("listing local network interfaces")?
        .iter()
        .map(|iface| iface.ip())
        .collect())
}

/// Within the `INNISFREE_SUBNET` parent range, return the first `/30`
/// whose two host addresses are both unbound on the local system.
/// `/30` is hardcoded because [`crate::wg::WireguardManager`] only ever
/// needs a pair of IPs (one local, one peer) per tunnel.
pub fn generate_unused_subnet() -> Result<IpNet> {
    let parent_net: IpNet = INNISFREE_SUBNET.parse()?;
    let in_use = local_addresses()?;
    for subnet in parent_net.subnets(30)? {
        // Skip the parent itself (/28), which `subnets()` yields first.
        if subnet.hosts().count() > 2 {
            continue;
        }
        if subnet.hosts().all(|h| !in_use.contains(&h)) {
            return Ok(subnet);
        }
    }
    Err(anyhow!("No available subnets within {}", parent_net))
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
