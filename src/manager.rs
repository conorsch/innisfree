use crate::config::{make_config_dir, ServicePort};
use crate::server::InnisfreeServer;
use crate::wg::WireguardManager;

#[derive(Debug)]
pub struct InnisfreeManager {
    ports: Vec<ServicePort>,
    dest_ip: String,
    floating_ip: Option<String>,
    pub server: InnisfreeServer,
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
        self.wg.wg_local_device.write_config();
        self.wait_for_cloudinit();
    }
    fn wait_for_cloudinit(&self) {
        debug!("Waiting for cloudinit to complete...");
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_cmd(cmd);
    }
    fn wait_for_ssh(&self) -> std::io::Result<()> {
        let mut dest_ip: String = self.server.ipv4_address();
        dest_ip.push_str(":22");
        loop {
            debug!("Waiting for ssh (polling socket {})...", dest_ip);
            let stream = std::net::TcpStream::connect(&dest_ip);
            match stream {
                Ok(_) => {
                    debug!("SSH port is open");
                    break;
                }
                Err(e) => {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                }
            }
        }
        Ok(())
    }
    pub fn known_hosts(&self) -> String {
        let ipv4_address = &self.server.ipv4_address();
        let server_host_key = &self.server.ssh_server_keypair.public;
        let mut host_line = ipv4_address.clone();
        host_line.push_str(" ");
        host_line.push_str(&server_host_key);

        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("known_hosts");
        std::fs::write(&fpath.to_str().unwrap(), host_line).expect("Failed to create known_hosts");
        return fpath.to_str().unwrap().to_string();
    }
    pub fn run_cmd(&self, cmd: Vec<&str>) {
        let ssh_kp = &self.server.ssh_client_keypair.filepath;
        let known_hosts = &self.known_hosts();
        let mut known_hosts_opt = "UserKnownHostsFile=".to_owned();
        known_hosts_opt.push_str(known_hosts);
        let ipv4_address = &self.server.ipv4_address();
        let mut cmd_args = vec![
            "-l",
            "innisfree",
            "-i",
            ssh_kp,
            "-o",
            &known_hosts_opt,
            ipv4_address,
        ];
        cmd_args.extend(cmd);
        std::process::Command::new("ssh")
            .args(cmd_args)
            .output()
            .expect("SSH command failed");
    }
}
