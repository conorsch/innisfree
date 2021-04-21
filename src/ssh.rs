use std::fs::{read_to_string, remove_file};
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct SSHKeypair {
    prefix: String,
    private: String,
    public: String,
    // TODO: type filepath as Path
    filepath: String,
}

pub fn create_ssh_keypair() -> SSHKeypair {
    // TODO: Use config dir, not /tmp
    let privkey_filepath: String = "/tmp/innisfree-ssh-key".to_string();
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
