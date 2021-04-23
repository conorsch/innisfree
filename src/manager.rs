use crate::wg::WireguardManager;
use crate::server:: InnisfreeServer;
use crate::config::{ServicePort, make_config_dir};

use std::fs::File;
use std::io::BufWriter;
use std::io::Write;


#[derive(Debug)]
pub struct InnisfreeManager {
    ports: Vec<ServicePort>,
    dest_ip: String,
    floating_ip: Option<String>,
    server: InnisfreeServer,
    wg: WireguardManager,
}


impl InnisfreeManager {
    pub fn new(ports: Vec<ServicePort>) -> InnisfreeManager {
        let wg = WireguardManager::new();
        let server = InnisfreeServer::new(ports, wg.wg_remote_device);
        InnisfreeManager {
            ports: server.services.to_vec(),
            dest_ip: "127.0.0.1".to_string(),
            floating_ip: Some("".to_string()),
            server: server,
            wg: WireguardManager::new(),
        }
    }
    pub fn up(&self) {
        self.wait_for_ssh();
        self.wait_for_cloudinit();
    }
    fn wait_for_cloudinit(&self) {
        debug!("Waiting for cloudinit (sleeping 20s)...");
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_cmd(cmd);
    }
    fn wait_for_ssh(&self) {
        debug!("Waiting for ssh (sleeping 40s)...");
        std::thread::sleep(std::time::Duration::from_secs(40));
    }
    pub fn known_hosts(&self) -> String {
        let ipv4_address = &self.server.ipv4_address();
        let server_host_key = &self.server.ssh_server_keypair.public;
        let mut host_line = ipv4_address.clone();
        host_line.push_str(&server_host_key);

        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("known_hosts");
        std::fs::write(&fpath.to_str().unwrap(), host_line).expect("Failed to create known_hosts");
        return fpath.to_str().unwrap().to_string();
    }
    pub fn run_cmd(&self, cmd: Vec<&str>) {
        let ssh_kp = &self.server.ssh_client_keypair.filepath;
        let known_hosts = &self.known_hosts();
        let ipv4_address = &self.server.ipv4_address();
        let mut cmd_args = vec![
            "-l",
            "innisfree",
            "-i",
            ssh_kp,
            "-o",
            known_hosts,
            ipv4_address,
        ];
        cmd_args.extend(cmd);
        let ssh_cmd = std::process::Command::new("ssh")
            .args(cmd_args)
            .output()
            .expect("SSH command failed");
    }
}
