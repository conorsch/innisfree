//! Utility functions for generating OpenSSH keypairs.
//! These keypairs are used for both client and server identities.
//! The client-side keys are written to a local config dir; the server
//! keys are placed inside a cloudinit YAML file and passed in during
//! instance creation.
//!
//! File naming and dir layout live in [`crate::state::TunnelStateDir`];
//! [`SshKeypair`] is just the key material and an opinionated writer
//! for a path the caller hands in.

use anyhow::{Context, Result};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, LineEnding, PrivateKey};
use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
/// Representation of an ED25519 SSH keypair.
pub struct SshKeypair {
    /// The private ED25519 key material.
    pub private: String,
    /// The public ED25519 key material.
    pub public: String,
}

impl SshKeypair {
    /// Generates a new ED25519 SSH keypair.
    pub fn new() -> Result<SshKeypair> {
        let kp = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)?;
        let privkey = kp.to_openssh(LineEnding::LF)?.to_string();
        let pubkey = kp.public_key().to_openssh()?;
        Ok(SshKeypair {
            private: privkey,
            public: pubkey,
        })
    }

    /// Write the keypair to disk. `private_key_path` receives the
    /// privkey at mode 0600; the matching pubkey is written to the
    /// same path with `.pub` appended at mode 0644. Caller (typically
    /// via [`crate::state::TunnelStateDir`]) owns naming and layout.
    pub fn write_to(&self, private_key_path: &Path) -> Result<()> {
        let mut privkey = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(private_key_path)
            .with_context(|| {
                format!(
                    "opening privkey path {} for writing",
                    private_key_path.display()
                )
            })?;
        privkey
            .write_all(self.private.as_bytes())
            .context("Failed to write SSH privkey")?;

        let pubkey_path = pub_path(private_key_path);
        let mut pubkey = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&pubkey_path)
            .with_context(|| {
                format!("opening pubkey path {} for writing", pubkey_path.display())
            })?;
        pubkey
            .write_all(self.public.as_bytes())
            .context("failed to write SSH pubkey")?;
        Ok(())
    }
}

/// Append `.pub` to a path, preserving any existing extension on the
/// stem (i.e. `client_id_ed25519` → `client_id_ed25519.pub`). Avoids
/// `Path::with_extension`, which would *replace* an extension if one
/// were ever present.
fn pub_path(private_key_path: &Path) -> PathBuf {
    let mut s: OsString = private_key_path.as_os_str().into();
    s.push(".pub");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_stripped() -> anyhow::Result<()> {
        let kp = SshKeypair::new()?;
        assert!(kp.private != kp.public);
        // trailing whitespace can screw up the yaml
        assert!(!kp.public.ends_with('\n'));
        assert!(!kp.public.ends_with(' '));
        // for privkey, that trailing newline is crucial.
        // lost an hour to debugging that
        assert!(kp.private.ends_with('\n'));
        Ok(())
    }

    #[test]
    fn pub_path_appends() {
        assert_eq!(
            pub_path(Path::new("/tmp/client_id_ed25519")),
            Path::new("/tmp/client_id_ed25519.pub")
        );
    }
}
