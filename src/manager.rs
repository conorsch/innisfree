use crate::config::{clean_config_dir, make_config_dir, ServicePort};
use crate::proxy::proxy_handler;
use crate::server::InnisfreeServer;
use crate::wg::WireguardManager;
use futures::future::join_all;

#[derive(Debug)]
pub struct InnisfreeManager {
    pub services: Vec<ServicePort>,
    dest_ip: String,
    floating_ip: Option<String>,
    pub server: InnisfreeServer,
    wg: WireguardManager,
}

impl InnisfreeManager {
    pub fn new(services: Vec<ServicePort>) -> InnisfreeManager {
        clean_config_dir();
        let wg = WireguardManager::new();
        let server = InnisfreeServer::new(services, wg.clone().wg_remote_device);
        InnisfreeManager {
            services: server.services.to_vec(),
            dest_ip: "127.0.0.1".to_string(),
            floating_ip: Some("".to_string()),
            server,
            wg,
        }
    }
    pub fn up(&self) {
        self.wait_for_ssh();
        debug!("Configuring remote proxy, creating tunnel...");
        self.wait_for_cloudinit();
        // Write out cloudinit config locally, for debugging
        // self.server.write_user_data();
        let ip = self.server.ipv4_address();
        let mut wg = self.wg.wg_local_device.clone();
        wg.peer.endpoint = ip;
        wg.write_config();
        debug!("Bringing up remote Wireguard interface");
        self.bring_up_remote_wg();
        debug!("Bringing up local Wireguard interface");
        self.bring_up_local_wg();
        self.test_connection();
    }
    fn wait_for_cloudinit(&self) {
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_cmd(cmd);
    }
    fn wait_for_ssh(&self) {
        let mut dest_ip: String = self.server.ipv4_address();
        dest_ip.push_str(":22");
        loop {
            let stream = std::net::TcpStream::connect(&dest_ip);
            match stream {
                Ok(_) => {
                    debug!("SSH port is open, proceeding");
                    break;
                }
                Err(_) => {
                    debug!("Waiting for ssh (polling socket {})...", dest_ip);
                    std::thread::sleep(std::time::Duration::from_secs(10));
                }
            }
        }
    }
    fn test_connection(&self) {
        let ip = &self.wg.wg_remote_ip;
        let cmd = "ping";
        let cmd_args = vec!["-c1", "-w5", &ip];
        let status = std::process::Command::new(cmd)
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(_) => {
                debug!("Confirmed tunnel is established, able to ping across it");
            }
            Err(_) => {
                error!("Failed to ping remote Wireguard interface, tunnel broken");
                // Unpleasant to panic here, should be returning results.
                assert!(status.unwrap().success());
            }
        }
    }
    pub fn bring_up_remote_wg(&self) {
        let cmd = vec!["wg-quick", "up", "/tmp/innisfree.conf"];
        self.run_cmd(cmd);
    }
    pub fn bring_up_local_wg(&self) {
        // Bring down in case the config was running with a different host
        self.bring_down_local_wg();
        let cmd = "wg-quick";
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("innisfree.conf");
        let cmd_args = vec!["up", &fpath.to_str().unwrap()];
        std::process::Command::new(cmd)
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("Failed to bring up local Wireguard interface");
    }
    pub fn bring_down_local_wg(&self) {
        let cmd = "wg-quick";
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("innisfree.conf");
        let cmd_args = vec!["down", &fpath.to_str().unwrap()];
        std::process::Command::new(cmd)
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("Failed to remove local Wireguard interface");
    }
    pub fn known_hosts(&self) -> String {
        let ipv4_address = &self.server.ipv4_address();
        let server_host_key = &self.server.ssh_server_keypair.public;
        let mut host_line = ipv4_address.clone();
        host_line.push(' ');
        host_line.push_str(&server_host_key);

        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("known_hosts");
        std::fs::write(&fpath.to_str().unwrap(), host_line).expect("Failed to create known_hosts");
        return fpath.to_str().unwrap().to_string();
    }
    pub fn run_cmd(&self, cmd: Vec<&str>) {
        let ssh_kp = &self.server.ssh_client_keypair.write_locally();
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
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .expect("SSH command failed");
    }
    pub fn clean(&self) {
        debug!("Removing local Wireguard interface");
        self.bring_down_local_wg();
        self.server.destroy();
    }
}

pub fn get_server_ip() -> Option<String> {
    let mut fpath = std::path::PathBuf::from(make_config_dir());
    fpath.push("known_hosts");
    let known_hosts = std::fs::read_to_string(&fpath).unwrap();
    let host_parts: Vec<&str> = known_hosts.split(' ').collect();
    let ip: String = host_parts[0].to_string();
    Some(ip)
}

pub fn open_shell() {
    let mut client_key = std::path::PathBuf::from(make_config_dir());
    client_key.push("client_id_ed25519");
    let mut known_hosts = std::path::PathBuf::from(make_config_dir());
    known_hosts.push("known_hosts");
    let mut known_hosts_opt = "UserKnownHostsFile=".to_owned();
    known_hosts_opt.push_str(known_hosts.to_str().unwrap());
    let ipv4_address = get_server_ip().unwrap();

    let cmd_args = vec![
        "-l",
        "innisfree",
        "-i",
        client_key.to_str().unwrap(),
        "-o",
        &known_hosts_opt,
        &ipv4_address,
    ];
    std::process::Command::new("ssh")
        .args(cmd_args)
        .status()
        .expect("SSH command failed");
}

pub async fn run_proxy(local_ip: String, dest_ip: String, services: Vec<ServicePort>) {
    // We'll kick off a dedicated proxy for each service,
    // and collect the handles to await them all together, concurrently.
    let mut tasks = vec![];
    for s in services {
        let listen_addr = format!("{}:{}", local_ip, s.port.clone());
        let dest_addr = format!("{}:{}", dest_ip, s.port.clone());
        let h = proxy_handler(listen_addr, dest_addr);
        // let ip = get_server_ip().unwrap();
        // debug!("Try accessing: {}:{} ({})", ip, s.port, s.protocol);
        tasks.push(h);
    }
    join_all(tasks).await;
    warn!("Join of all service proxies returned, surprisingly");
}
