extern crate custom_error;
use custom_error::custom_error;

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
    IpNetAssignment{source: ipnet::AddrParseError} = "Failed to find unclaimed IP address",
    IpAddrAssignment{source: std::net::AddrParseError} = "Failed to find unclaimed IP address",
    ApiError{source: serde_json::Error} = "Could not parse JSON from API response",
    ConfigError{source: std::env::VarError} = "DigitalOcean API token not set",
    Unknown = "unknown error",
}
