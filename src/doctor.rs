//! Pre-flight checks for the local environment.
//!
//! Validates that the binary has the privileges it needs to bring up
//! a Wireguard interface in-process via boringtun, and that the
//! cloud-provider credentials are available.

use anyhow::Result;
use std::fs;

/// Linux capability bit for `CAP_NET_ADMIN`. See `capabilities(7)`.
const CAP_NET_ADMIN: u64 = 12;

/// Run all platform checks. Returns `Ok(true)` if everything looks
/// good, `Ok(false)` if any non-fatal check failed (logged via
/// `tracing::warn!`).
pub fn platform_is_supported() -> Result<bool> {
    let mut ok = true;

    if has_cap_net_admin()? {
        tracing::info!("CAP_NET_ADMIN is held — local Wireguard interface can be created");
    } else {
        tracing::warn!(
            "CAP_NET_ADMIN is not held — boringtun cannot open /dev/net/tun. \
             Fix with one of:\n  \
             setcap cap_net_admin+ep $(which innisfree)\n  \
             systemd: AmbientCapabilities=CAP_NET_ADMIN in the unit file"
        );
        ok = false;
    }

    if std::env::var("DIGITALOCEAN_API_TOKEN").is_ok() {
        tracing::info!("DIGITALOCEAN_API_TOKEN is set");
    } else {
        tracing::warn!("DIGITALOCEAN_API_TOKEN is not set — server provisioning will fail");
        ok = false;
    }

    Ok(ok)
}

/// Read the effective capability set from `/proc/self/status` and
/// return whether `CAP_NET_ADMIN` is held. Avoids pulling in the full
/// `caps` crate for a one-bit check.
fn has_cap_net_admin() -> Result<bool> {
    let status = fs::read_to_string("/proc/self/status")?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("CapEff:\t") {
            let bits = u64::from_str_radix(rest.trim(), 16)?;
            return Ok(bits & (1u64 << CAP_NET_ADMIN) != 0);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_check_does_not_panic() {
        // We don't assert the result — CI may or may not grant the
        // capability — only that the read+parse path is sound.
        let _ = has_cap_net_admin().unwrap();
    }
}
