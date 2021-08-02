use crate::config::{clean_config_dir, make_config_dir, InnisfreeError, ServicePort};
use crate::proxy::proxy_handler;
use crate::server::InnisfreeServer;
use crate::wg::WireguardManager;
use futures::future::join_all;
use tokio::signal;

#[derive(Debug)]
pub struct InnisfreeManager {
    pub services: Vec<ServicePort>,
    dest_ip: String,
    floating_ip: Option<String>,
    pub server: InnisfreeServer,
    pub name: String,
    wg: WireguardManager,
}

impl InnisfreeManager {
    pub async fn new(
        tunnel_name: &str,
        services: Vec<ServicePort>,
    ) -> Result<InnisfreeManager, InnisfreeError> {
        clean_config_dir();
        let wg = WireguardManager::new()?;
        let server =
            InnisfreeServer::new(tunnel_name, services, wg.clone().wg_remote_device).await?;
        Ok(InnisfreeManager {
            name: tunnel_name.to_string(),
            services: server.services.to_vec(),
            dest_ip: "127.0.0.1".to_string(),
            floating_ip: Some("".to_string()),
            server,
            wg,
        })
    }
    pub fn up(&self) -> Result<(), InnisfreeError> {
        self.wait_for_ssh();
        debug!("Configuring remote proxy and opening tunnel...");
        self.wait_for_cloudinit()?;
        // Write out cloudinit config locally, for debugging
        // self.server.write_user_data();
        let ip = self.server.ipv4_address();
        let mut wg = self.wg.wg_local_device.clone();
        wg.peer.endpoint = ip;
        wg.write_locally(&self.services);
        debug!("Bringing up remote Wireguard interface");
        self.bring_up_remote_wg()?;
        debug!("Bringing up local Wireguard interface");
        self.bring_up_local_wg()?;
        trace!("Testing connection");
        self.test_connection()
    }
    fn wait_for_cloudinit(&self) -> Result<(), InnisfreeError> {
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_ssh_cmd(cmd)
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
                    debug!("Waiting for ssh...");
                    trace!("Polling socket {})...", dest_ip);
                    std::thread::sleep(std::time::Duration::from_secs(10));
                }
            }
        }
    }
    /// Wait for an interrupt signal, then terminate gracefully,
    /// cleaning up droplet resources before exit.
    pub async fn block(&self) {
        match signal::ctrl_c().await {
            Ok(()) => {
                warn!("Received stop signal, exiting gracefully");
                self.clean().await;
                debug!("Clean up complete, exiting!");
                std::process::exit(0);
            }
            Err(e) => {
                error!("Unable to register hook for ctrl+c: {}", e);
                std::process::exit(10);
            }
        }
    }
    fn test_connection(&self) -> Result<(), InnisfreeError> {
        trace!("Inside test connection, setting up vars");
        let ip = &self.wg.wg_remote_ip;
        trace!("Inside test connection, running ping cmd");
        let status = std::process::Command::new("ping")
            .arg("-c1")
            .arg("-w5")
            .arg(&ip)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        trace!("Inside test connection, evaluating match");
        match status {
            Ok(s) => {
                if s.success() {
                    debug!("Confirmed tunnel is established, able to ping across it");
                    Ok(())
                } else {
                    Err(InnisfreeError::CommandFailure {
                        msg: "Failed to ping remote Wireguard interface, tunnel broken".to_string(),
                    })
                }
            }
            Err(_) => {
                trace!("Inside test connection match, OK, failure");
                Err(InnisfreeError::CommandFailure {
                    msg: "Failed to ping remote Wireguard interface, tunnel broken".to_string(),
                })
            }
        }
    }
    pub fn bring_up_remote_wg(&self) -> Result<(), InnisfreeError> {
        let cmd = vec!["wg-quick", "up", "/tmp/innisfree.conf"];
        trace!("Activating remote wg interface");
        self.run_ssh_cmd(cmd)
    }
    pub fn bring_up_local_wg(&self) -> Result<(), InnisfreeError> {
        trace!("Bringing up local wg conn");
        // Bring down in case the config was running with a different host
        let _down = self.bring_down_local_wg();
        trace!("Building path to local wg config");
        // Bring down in case the config was running with a different host
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("innisfree.conf");
        trace!("Running local wg-quick cmd");
        let result = std::process::Command::new("wg-quick")
            .arg("up")
            .arg(&fpath.to_str().unwrap())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        trace!("Inspecting wg-quick results: pre-match");
        match result {
            Ok(r) => {
                trace!("Inspecting wg-quick results: inside OK");
                if r.success() {
                    trace!("Inspecting wg-quick results: inside OK, inside success");
                    Ok(())
                } else {
                    trace!("Inspecting wg-quick results: inside OK, inside failure");
                    Err(InnisfreeError::CommandFailure {
                        msg: "Failed to bring up local Wireguard interface".to_string(),
                    })
                }
            }
            Err(_) => Err(InnisfreeError::CommandFailure {
                msg: "Failed to bring up local Wireguard interface".to_string(),
            }),
        }
    }
    pub fn bring_down_local_wg(&self) -> Result<(), InnisfreeError> {
        let cmd = "wg-quick";
        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("innisfree.conf");
        let cmd_args = vec!["down", fpath.to_str().unwrap()];
        let result = std::process::Command::new(cmd)
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match result {
            Ok(r) => {
                if r.success() {
                    Ok(())
                } else {
                    Err(InnisfreeError::CommandFailure {
                        msg: "Failed to remove local Wireguard interface".to_string(),
                    })
                }
            }
            Err(_) => Err(InnisfreeError::CommandFailure {
                msg: "Failed to remove local Wireguard interface".to_string(),
            }),
        }
    }
    pub fn known_hosts(&self) -> String {
        let ipv4_address = &self.server.ipv4_address();
        let server_host_key = &self.server.ssh_server_keypair.public;
        let mut host_line = ipv4_address.to_owned();
        host_line.push(' ');
        host_line.push_str(server_host_key);

        let mut fpath = std::path::PathBuf::from(make_config_dir());
        fpath.push("known_hosts");
        std::fs::write(&fpath.to_str().unwrap(), host_line).expect("Failed to create known_hosts");
        return fpath.to_str().unwrap().to_string();
    }
    pub fn run_ssh_cmd(&self, cmd: Vec<&str>) -> Result<(), InnisfreeError> {
        trace!("Entering run_ssh_cmd");
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
            "-o",
            "ConnectTimeout=5",
            ipv4_address,
        ];
        cmd_args.extend(cmd);
        let status = std::process::Command::new("ssh")
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) => {
                trace!("SSH command ran, checking status code of returned process");
                if s.success() {
                    trace!("Yes, process was truly successful");
                    Ok(())
                } else {
                    error!("SSH command failed: {}", s);
                    Err(InnisfreeError::SshCommandFailure)
                }
            }
            Err(e) => {
                error!("SSH command failed: {}", e);
                Err(InnisfreeError::SshCommandFailure)
            }
        }
    }
    pub async fn clean(&self) {
        debug!("Removing local Wireguard interface");
        // Ignore errors, since we want to try all handlers
        let _ = self.bring_down_local_wg();
        let _ = self.server.destroy().await;
    }
    pub async fn assign_floating_ip(&self, floating_ip: &str) {
        self.server.assign_floating_ip(floating_ip).await;
    }
}

pub fn get_server_ip() -> Result<String, std::io::Error> {
    trace!("Looking up server IP from known_hosts file");
    let mut fpath = std::path::PathBuf::from(make_config_dir());
    fpath.push("known_hosts");
    let known_hosts = std::fs::read_to_string(&fpath)?;
    let host_parts: Vec<&str> = known_hosts.split(' ').collect();
    let ip: String = host_parts[0].to_string();
    Ok(ip)
}

pub fn open_shell() -> Result<(), std::io::Error> {
    let mut client_key = std::path::PathBuf::from(make_config_dir());
    client_key.push("client_id_ed25519");
    let mut known_hosts = std::path::PathBuf::from(make_config_dir());
    known_hosts.push("known_hosts");
    let mut known_hosts_opt = "UserKnownHostsFile=".to_owned();
    known_hosts_opt.push_str(known_hosts.to_str().unwrap());
    let ipv4_address = match get_server_ip() {
        Ok(ip) => ip,
        Err(e) => {
            return Err(e);
        }
    };
    let cmd_args = vec![
        "-l",
        "innisfree",
        "-i",
        client_key.to_str().unwrap(),
        "-o",
        &known_hosts_opt,
        "-o",
        "ConnectTimeout=5",
        &ipv4_address,
    ];
    let status = std::process::Command::new("ssh").args(cmd_args).status();
    match status {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("SSH interactive session failed: {}", e);
            Err(e)
        }
    }
}

pub async fn run_proxy(
    local_ip: String,
    dest_ip: String,
    services: Vec<ServicePort>,
) -> Result<(), InnisfreeError> {
    // We'll kick off a dedicated proxy for each service,
    // and collect the handles to await them all together, concurrently.
    let mut tasks = vec![];
    for s in services {
        let listen_addr = format!("{}:{}", local_ip, &s.port);
        let dest_addr = format!("{}:{}", dest_ip, &s.port);
        let h = proxy_handler(listen_addr, dest_addr);
        // let ip = get_server_ip().unwrap();
        // debug!("Try accessing: {}:{} ({})", ip, s.port, s.protocol);
        tasks.push(h);
    }
    // We expect the proxies to block indefinitely, except ctrl+c.
    // If they return earlier, we'll be able to inspect the errors.
    let proxy_tasks = join_all(tasks).await;
    warn!("Proxy stopped unexpectedly, no longer forwarding traffic");
    for t in proxy_tasks {
        match t {
            Ok(t) => {
                // I don't expect to see this
                debug!("Service proxy returned ok: {:?}", t);
            }
            Err(e) => {
                error!("Service proxy failed: {}", e);
                return Err(InnisfreeError::Unknown);
            }
        }
    }
    Ok(())
}
