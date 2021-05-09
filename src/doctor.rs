use crate::config::InnisfreeError;

const OS_RELEASE: &str = "/etc/os-release";

pub fn is_linux() -> Result<bool, InnisfreeError> {
    if std::path::Path::new(OS_RELEASE).exists() {
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn platform_is_supported() -> Result<bool, InnisfreeError> {
    let result = match std::env::var("DIGITALOCEAN_API_TOKEN") {
        Ok(_) => {
            info!("DIGITALOCEAN_API_TOKEN set, can create resources");
            true
        }
        Err(_) => {
            warn!("DIGITALOCEAN_API_TOKEN not set, cannot create resources");
            false
        }
    };
    if check_if_command_exists("wg-quick").unwrap() {
        info!("Wireguard appears to be installed!");
        Ok(result)
    } else {
        warn!("Wireguard does not appear to be installed");
        // Spit out some more helpful info
        match distro_support() {
            Ok(_) => Ok(result),
            Err(e) => Err(e),
        }
    }
}

pub fn distro_support() -> Result<bool, InnisfreeError> {
    if !is_linux().unwrap() {
        return Err(InnisfreeError::PlatformError);
    }
    let os_release = std::fs::read_to_string(OS_RELEASE)?.trim().to_string();
    // These checks will bit-rot fast, since they're naive comparisons,
    // and don't perform >= checks on the version_id. Mostly we just
    // need to wait until Debian Stable 11 Bullseye is released,
    // then pretty much every Linux distro will support Wireguard out of the box.
    if os_release.contains("ID=debian") && os_release.contains("VERSION_CODENAME=buster") {
        info!(
            "Debian Stable Buster 10 doesn't ship wireguard by default, but it's available
              in buster-backports. See for details: https://www.wireguard.com/install/"
        );
    } else if os_release.contains("ID=ubuntu") && os_release.contains("VERSION_CODENAME=focal") {
        info!(
            "Ubuntu Focal 20.04 supports Wireguard out of the box. \
              Run 'apt-get install wireguard wireguard-tools'."
        );
    } else if os_release.contains("ID=fedora") && os_release.contains("VERSION_CODENAME=33") {
        info!(
            "Fedora 33 supports Wireguard out of the box. \
              Run 'dnf install wireguard wireguard-tools'."
        );
    } else {
        warn!(
            "Unknown support for your platform. See official Wireguard \
              install docs at https://www.wireguard.com/install/"
        );
    }
    Ok(true)
}

pub fn check_if_command_exists(cmd: &str) -> Result<bool, InnisfreeError> {
    match std::process::Command::new(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => Ok(true),
        // All we care about is whether we found the command,
        // so treat all errors as "nope".
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wireguard_exists() {
        assert!(check_if_command_exists("wg-quick").unwrap());
    }

    #[test]
    fn missing_cmd_does_not_exist() {
        assert!(!check_if_command_exists("wg-quick2").unwrap());
    }

    #[test]
    fn platform_is_linux() {
        assert!(is_linux().unwrap());
    }
}
