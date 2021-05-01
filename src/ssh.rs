use std::fs::read_to_string;
use std::path::Path;
use std::process::Command;

extern crate tempfile;

use crate::config::make_config_dir;

#[derive(Debug)]
pub struct SshKeypair {
    prefix: String,
    pub private: String,
    pub public: String,
}

impl SshKeypair {
    pub fn new(prefix: &str) -> SshKeypair {
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
    pub fn write_locally(&self) -> String {
        let config_dir = make_config_dir();
        let key_name = self.filename();
        let privkey_filepath: String = Path::new(&config_dir)
            .join(key_name)
            .to_str()
            .unwrap()
            .to_string();
        let pubkey_filepath: String = privkey_filepath.clone() + ".pub";
        std::fs::write(&privkey_filepath, &self.private).expect("Failed to write SSH privkey");
        std::fs::write(&pubkey_filepath, &self.private).expect("Failed to write SSH privkey");
        privkey_filepath
    }
}

fn create_ssh_keypair(prefix: &str) -> SshKeypair {
    // Really clumsy with Path & PathBuf, so converting everything to Strings for now
    let tmpfile = tempfile::NamedTempFile::new().unwrap();
    let tmpfile = tmpfile.path();
    let privkey_filepath = String::from(tmpfile.to_str().unwrap());
    let pubkey_filepath: String = privkey_filepath.clone() + ".pub";

    // ssh-keygen won't clobber, requires interactive 'y' to confirm.
    // so delete the file beforehand, then it'll create happily.
    // tempfile will still be cleaned up when dropped
    if Path::new(&privkey_filepath).exists() {
        let _ = std::fs::remove_file(&privkey_filepath);
    }

    Command::new("ssh-keygen")
        .args(&[
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
        .status()
        .expect("failed to generate ssh keypair via ssh-keygen");

    let privkey = read_to_string(&privkey_filepath).expect("Failed to open ssh privkey file");
    let pubkey = read_to_string(&pubkey_filepath)
        .expect("Failed to open ssh pubkey file")
        .trim()
        .to_string();
    SshKeypair {
        prefix: prefix.to_string(),
        private: privkey,
        public: pubkey,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_is_stripped() {
        let kp = SshKeypair::new("test1");
        assert!(kp.private != kp.public);
        // trailing whitespace can screw up the yaml
        assert!(!kp.public.ends_with("\n"));
        assert!(!kp.public.ends_with(" "));
        // for privkey, that trailing newline is crucial.
        // lost an hour to debugging that
        assert!(kp.private.ends_with("\n"));
    }
}
