//! Utility functions for generating OpenSSH keypairs.
//! These keypairs are used for both client and server identities.
//! The client-side keys are written to a local config dir,
//! by default `~/.config/innisfree/<service>`; the server
//! keys are placed inside a cloudinit YAML file and passed in
//! during instance creation.

use crate::config::make_config_dir;
use anyhow::{Context, Result};
use osshkeys::cipher::Cipher;
use osshkeys::keys::{KeyPair, KeyType};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
/// Representation of an ED25519 SSH keypair.
pub struct SshKeypair {
    /// A human-readable prefix to distinguish it with a unique
    /// filepath if and when its written to disk.
    prefix: String,
    /// The private ED25519 key material.
    pub private: String,
    /// The public ED25519 key material.
    pub public: String,
}

impl SshKeypair {
    /// Generates a new ED25519 SSH keypair.
    pub fn new(prefix: &str) -> Result<SshKeypair> {
        let kp = KeyPair::generate(KeyType::ED25519, 0)?;
        let privkey = kp.serialize_openssh(None, Cipher::Null)?;
        let pubkey = kp.serialize_publickey()?;
        Ok(SshKeypair {
            prefix: prefix.to_string(),
            private: privkey,
            public: pubkey,
        })
    }

    /// Builds predictable filename, based on the prefix,
    /// for use in writing to disk.
    fn filename(&self) -> String {
        format!("{}_{}", &self.prefix, "id_ed25519")
    }

    /// Store keypair on disk, in config dir.
    pub fn write_locally(&self, service_name: &str) -> Result<PathBuf> {
        tracing::trace!("writing ssh keypair locally");
        // Ensure service config dir is present
        let config_dir = make_config_dir(service_name).context("failed to create config dir")?;

        // Write SSH privkey.
        let privkey_filepath = Path::new(&config_dir).join(&self.filename());
        let mut privkey = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&privkey_filepath)
            .context("failed to open privkey filepath for writing")?;
        privkey
            .write_all(&self.private.as_bytes())
            .context("Failed to write SSH privkey")?;

        // Write SSH pubkey.
        let pubkey_filepath = Path::new(&config_dir).join(format!("{}.pub", &self.filename()));
        let mut pubkey = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o644)
            .open(&pubkey_filepath)
            .context("failed to open pubkey filepath for writing")?;
        pubkey
            .write_all(&self.public.as_bytes())
            .context("failed to write SSH pubkey")?;
        Ok(privkey_filepath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_stripped() -> anyhow::Result<()> {
        let kp = SshKeypair::new("test1")?;
        assert!(kp.private != kp.public);
        // trailing whitespace can screw up the yaml
        assert!(!kp.public.ends_with('\n'));
        assert!(!kp.public.ends_with(' '));
        // for privkey, that trailing newline is crucial.
        // lost an hour to debugging that
        assert!(kp.private.ends_with('\n'));
        Ok(())
    }
}
