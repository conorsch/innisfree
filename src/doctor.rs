//! Utility functions for detecting dependencies.
//! Checks whether Wireguard is installed, whether
//! a cloud provider authorization token is present.

use anyhow::Result;

/// Checks that `wg-quick` is found on `$PATH`.
/// Also checks that `DIGITALOCEAN_API_TOKEN` environment
/// variable is set.
pub fn platform_is_supported() -> Result<bool> {
    let mut result: bool = std::env::var("DIGITALOCEAN_API_TOKEN").is_ok();
    if check_if_command_exists("wg-quick") {
        tracing::info!("Wireguard appears to be installed!");
    } else {
        tracing::warn!("Wireguard does not appear to be installed");
        result = false;
    }
    Ok(result)
}

/// Search for given program on `$PATH`.
fn check_if_command_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        // All we care about is whether we found the command,
        // so treat all errors as "nope".
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wireguard_exists() {
        assert!(check_if_command_exists("wg-quick"));
    }

    #[test]
    fn missing_cmd_does_not_exist() {
        assert!(!check_if_command_exists("wg-quick2"));
    }
}
