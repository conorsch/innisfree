use anyhow::{Context, Result};
use std::fs::read_to_string;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

use std::io::Write;

extern crate tempfile;

use crate::config::make_config_dir;

#[derive(Debug)]
pub struct SshKeypair {
    prefix: String,
    pub private: String,
    pub public: String,
}

impl SshKeypair {
    pub fn new(prefix: &str) -> Result<SshKeypair> {
        create_ssh_keypair(prefix)
    }

    // Builds predictable filename for use in writing
    fn filename(&self) -> String {
        let mut key_name = String::from(&self.prefix);
        key_name.push('_');
        key_name.push_str("id_ed25519");
        key_name
    }
    // Store keypair on disk, in config dir
    pub fn write_locally(&self, service_name: &str) -> Result<String> {
        let config_dir = make_config_dir(service_name);
        let key_name = self.filename();
        let privkey_filepath: String = Path::new(&config_dir)
            .join(key_name)
            .to_str()
            .unwrap()
            .to_string();

        // From the dope https://github.com/Leo1003/rust-osshkeys/blob/master/examples/generate_keyfile.rs
        // which bizarrely I am not yet using, found it after writing the shell-outs version
        // already
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
        std::fs::write(&pubkey_filepath, &self.public)
            .map_err(|e| anyhow::Error::new(e).context("Failed to write SSH pubkey"))?;
        Ok(privkey_filepath)
    }
}

fn create_ssh_keypair(prefix: &str) -> Result<SshKeypair> {
    // Really clumsy with Path & PathBuf, so converting everything to Strings for now
    let tmpfile = tempfile::NamedTempFile::new()?;
    let tmpfile = tmpfile.path();
    let privkey_filepath = String::from(tmpfile.to_str().unwrap());
    let pubkey_filepath = String::from(&privkey_filepath) + ".pub";

    // ssh-keygen won't clobber, requires interactive 'y' to confirm.
    // so delete the file beforehand, then it'll create happily.
    // tempfile will still be cleaned up when dropped
    if Path::new(&privkey_filepath).exists() {
        std::fs::remove_file(&privkey_filepath)?;
    }

    let status = Command::new("ssh-keygen")
        .args([
            "-t",
            "ed25519",
            "-P",
            "",
            "-f",
            &privkey_filepath,
            "-C",
            "",
            "-q",
        ])
        .status();
    match status {
        Ok(_) => {}
        Err(e) => {
            return Err(anyhow::Error::new(e).context("Failed to generate SSH keypair"));
        }
    }

    let privkey = read_to_string(&privkey_filepath)?;
    let pubkey = read_to_string(&pubkey_filepath)?.trim().to_string();
    Ok(SshKeypair {
        prefix: prefix.to_string(),
        private: privkey,
        public: pubkey,
    })
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
