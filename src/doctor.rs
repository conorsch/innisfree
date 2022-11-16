use anyhow::Result;

pub fn platform_is_supported() -> Result<bool> {
    let mut result: bool = std::env::var("DIGITALOCEAN_API_TOKEN").is_ok();
    if check_if_command_exists("wg-quick")? {
        info!("Wireguard appears to be installed!");
    } else {
        warn!("Wireguard does not appear to be installed");
        result = false;
    }
    Ok(result)
}

pub fn check_if_command_exists(cmd: &str) -> Result<bool> {
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
}
