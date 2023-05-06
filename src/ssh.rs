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
use std::path::Path;

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
        let mut key_name = String::from(&self.prefix);
        key_name.push('_');
        key_name.push_str("id_ed25519");
        key_name
    }
    /// Store keypair on disk, in config dir.
    pub fn write_locally(&self, service_name: &str) -> Result<String> {
        let config_dir = make_config_dir(service_name)?;
        let key_name = self.filename();
        let privkey_filepath: String = Path::new(&config_dir)
            .join(key_name)
            .to_str()
            .unwrap()
            .to_string();

        let mut fop = std::fs::OpenOptions::new();
        fop.write(true).create(true).truncate(true);
        // cfg_if! requires external crate, look into it
        // cfg_if! {
        //    if #[cfg(unix)] {
        //        fop.mode(0o600);
        //    }
        // }
        fop.mode(0o600);
        let mut f = fop.open(&privkey_filepath)?;
        // std::fs::write(&privkey_filepath, &self.private).expect("Failed to write SSH privkey");
        f.write_all(self.private.as_bytes())
            .context("Failed to write SSH privkey")?;

        // Pubkey is public, so default umask is fine, expecting 644 or so.
        let pubkey_filepath = String::from(&privkey_filepath) + ".pub";
        std::fs::write(pubkey_filepath, &self.public)
            .map_err(|e| anyhow::Error::new(e).context("Failed to write SSH pubkey"))?;
        Ok(privkey_filepath)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_stripped() {
        let kp = SshKeypair::new("test1").unwrap();
        assert!(kp.private != kp.public);
        // trailing whitespace can screw up the yaml
        assert!(!kp.public.ends_with('\n'));
        assert!(!kp.public.ends_with(' '));
        // for privkey, that trailing newline is crucial.
        // lost an hour to debugging that
        assert!(kp.private.ends_with('\n'));
    }
}
