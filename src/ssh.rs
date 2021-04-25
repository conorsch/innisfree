use std::fs::{read_to_string, remove_file};
use std::path::Path;
use std::process::Command;

use crate::config::make_config_dir;

#[derive(Debug)]
pub struct SshKeypair {
    prefix: String,
    pub private: String,
    pub public: String,
    // TODO: type filepath as Path
    pub filepath: String,
}

impl SshKeypair {
    pub fn new(prefix: &str) -> SshKeypair {
        create_ssh_keypair(prefix)
    }
}

fn create_ssh_keypair(prefix: &str) -> SshKeypair {
    // Really clumsy with Path & PathBuf, so converting everything to Strings for now
    let config_dir = make_config_dir();
    let mut key_name = String::from(prefix);
    key_name.push('_');
    key_name.push_str("id_ed25519");
    let privkey_filepath: String = Path::new(&config_dir)
        .join(key_name)
        .to_str()
        .unwrap()
        .to_string();
    let pubkey_filepath: String = privkey_filepath.clone() + ".pub";
    if Path::new(&privkey_filepath).exists() {
        let _ = remove_file(&privkey_filepath);
    }
    if Path::new(&pubkey_filepath).exists() {
        let _ = remove_file(&pubkey_filepath);
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
        filepath: privkey_filepath,
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
