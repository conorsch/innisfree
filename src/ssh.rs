use std::fs::{read_to_string, remove_file};
use std::path::Path;
use std::process::Command;

use crate::config::make_config_dir;

#[derive(Debug)]
pub struct SSHKeypair {
    prefix: String,
    pub private: String,
    pub public: String,
    // TODO: type filepath as Path
    pub filepath: String,
}

impl SSHKeypair {
    pub fn new() -> SSHKeypair {
        create_ssh_keypair()
    }
}

fn create_ssh_keypair() -> SSHKeypair {
    // Really clumsy with Path & PathBuf, so converting everything to Strings for now
    let config_dir = make_config_dir();
    let privkey_filepath: String = Path::new(&config_dir)
        .join("innisfree-ssh-key")
        .to_str()
        .unwrap()
        .to_string();
    let pubkey_filepath: String = privkey_filepath.clone() + ".pub";
    debug!("Removing pre-existing ssh key files...");
    if Path::new(&privkey_filepath).exists() {
        let _ = remove_file(&privkey_filepath);
    }
    if Path::new(&pubkey_filepath).exists() {
        let _ = remove_file(&pubkey_filepath);
    }

    debug!("Generating new keys via ssh-keygen...");
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
    let pubkey = read_to_string(&pubkey_filepath).expect("Failed to open ssh pubkey file");
    let kp = SSHKeypair {
        prefix: "".to_string(),
        private: privkey,
        public: pubkey,
        filepath: privkey_filepath,
    };
    return kp;
}
