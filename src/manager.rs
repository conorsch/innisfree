//! High-level controller logic for managing
//! service proxies, i.e. [TunnelManager].

use crate::config::{clean_config_dir, make_config_dir, ServicePort};

use crate::proxy::proxy_handler;
use crate::server::digitalocean::server::Droplet;
use crate::server::InnisfreeServer;
use crate::ssh::SshKeypair;
use crate::wg::WireguardManager;
use anyhow::{anyhow, Context, Result};
use futures::future::join_all;
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::PathBuf;
use tokio::signal;

/// Controller class for handling tunnel configurations.
/// Handles the soup-to-nuts configuration, including server creation,
/// WireGuard device config, and proxy.
pub struct TunnelManager {
    /// List of `ServicePort`s to manage connections for.
    pub services: Vec<ServicePort>,
    // dest_ip: IpAddr,
    /// Remote server handling public ingress.
    pub server: Box<dyn InnisfreeServer>,
    /// Human-readable name for this service manager.
    pub name: String,
    /// Controller for Wireguard tunnels.
    pub wg: WireguardManager,
    /// SSH keypair for managing client-side SSH connections.
    pub ssh_client_keypair: SshKeypair,
    /// SSH keypair for identifying remote SSH server identity.
    pub ssh_server_keypair: SshKeypair,
    /// Static IP to be attached to the server, for stable DNS entries on recreation.
    pub static_ip: Option<IpAddr>,
}

impl TunnelManager {
    /// Create a new controller for managing a collection of services.
    /// Call `up()` to build.
    pub async fn new(
        tunnel_name: &str,
        services: Vec<ServicePort>,
        static_ip: Option<IpAddr>,
    ) -> Result<TunnelManager> {
        clean_config_dir(tunnel_name)?;
        let wg = WireguardManager::new(tunnel_name)?;
        // Create new ephemeral ssh keypair
        let ssh_client_keypair = SshKeypair::new("client")?;
        let ssh_server_keypair = SshKeypair::new("server")?;
        let server = Droplet::new(
            tunnel_name,
            services.clone(),
            wg.clone(),
            &ssh_client_keypair,
            &ssh_server_keypair,
        )
        .await?;

        if let Some(ip) = static_ip {
            server.assign_floating_ip(ip).await?;
        }

        Ok(TunnelManager {
            name: tunnel_name.to_owned(),
            services,
            server: Box::new(server),
            ssh_client_keypair,
            ssh_server_keypair,
            static_ip,
            wg,
        })
    }
    /// Create remote and local infrastructure. Creates a cloud server,
    /// configures it to forward public ports over its Wireguard interface,
    /// to a local Wireguard interface
    pub fn up(&self) -> Result<()> {
        self.wait_for_ssh()?;
        tracing::debug!("Configuring remote proxy...");
        self.wait_for_cloudinit()
            .context("failed while waiting for cloudinit")?;
        // Write out cloudinit config locally, for debugging
        // self.server.write_user_data();
        let ip = self.server.ipv4_address()?;
        tracing::debug!("Configuring tunnel...");
        let mut wg = self.wg.wg_local_device.clone();
        wg.peer.endpoint = Some(ip);
        wg.write_locally(&self.name, &self.services)
            .context("failed to write wireguard configs")?;
        tracing::debug!("Bringing up remote Wireguard interface");
        self.bring_up_remote_wg()
            .context("failed to bring up remote wg interface")?;
        tracing::debug!("Bringing up local Wireguard interface");
        self.bring_up_local_wg()
            .context("failed to bring up local wg interface")?;

        tracing::trace!("Testing connection");
        self.test_connection()
    }
    /// Blocks until the server's cloudinit process reports completion.
    fn wait_for_cloudinit(&self) -> Result<()> {
        let cmd: Vec<&str> = vec!["cloud-init", "status", "--long", "--wait"];
        self.run_ssh_cmd(cmd)
    }
    /// Blocks until 22/TCP is available on the server.
    fn wait_for_ssh(&self) -> Result<()> {
        let dest_ip = SocketAddr::new(self.server.ipv4_address()?, 22);
        loop {
            let stream = TcpStream::connect(dest_ip);
            match stream {
                Ok(_) => {
                    tracing::debug!("SSH port is open, proceeding");
                    break;
                }
                Err(_) => {
                    tracing::debug!("Waiting for ssh...");
                    tracing::trace!("Polling socket {})...", dest_ip);
                    std::thread::sleep(std::time::Duration::from_secs(10));
                }
            }
        }
        Ok(())
    }
    /// Wait for an interrupt signal, then terminate gracefully,
    /// cleaning up droplet resources before exit.
    pub async fn block(&self) -> Result<()> {
        match signal::ctrl_c().await {
            Ok(()) => {
                tracing::warn!("Received stop signal, exiting gracefully");
                self.clean().await?;
                tracing::info!("Clean up complete, exiting");
                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!("Unable to register hook for ctrl+c: {}", e);
                std::process::exit(10);
            }
        }
    }
    /// Ping remote remote Wireguard IP from local Wireguard device.
    /// Ensures connectivity is established between remote and local interfaces.
    fn test_connection(&self) -> Result<()> {
        tracing::trace!("Inside test connection, setting up vars");
        let ip = &self.wg.wg_remote_ip;
        tracing::trace!("Inside test connection, running ping cmd");
        std::process::Command::new("ping")
            .arg("-c1")
            .arg("-w5")
            .arg(&ip.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Failed to ping remote Wireguard interface, tunnel broken")?;
        tracing::debug!("Confirmed tunnel is established, able to ping across it");
        Ok(())
    }
    /// Returns `PathBuf`, creating directory if necessary.
    fn config_dir(&self) -> Result<PathBuf> {
        make_config_dir(&self.name)
    }
    /// Runs `wg-quick up` on remote server to bring up its Wireguard interface.
    fn bring_up_remote_wg(&self) -> Result<()> {
        let cmd = vec!["wg-quick", "up", "/tmp/innisfree.conf"];
        tracing::trace!("Activating remote wg interface");
        self.run_ssh_cmd(cmd)
    }
    /// Runs `wg-quick up` on localhost to bring up local Wireguard interface.
    fn bring_up_local_wg(&self) -> Result<()> {
        tracing::trace!("Bringing up local wg conn");
        // Bring down in case the config was running with a different host
        let _down = self.bring_down_local_wg();
        tracing::trace!("Building path to local wg config");
        // Bring down in case the config was running with a different host
        let mut fpath = std::path::PathBuf::from(&self.config_dir()?);
        fpath.push(format!("{}.conf", &self.name));
        tracing::trace!("Running local wg-quick cmd");
        std::process::Command::new("wg-quick")
            .arg("up")
            .arg(fpath.display().to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        Ok(())
    }
    /// Run `wg-quick down` on localhost to destroy local Wireguard interface.
    fn bring_down_local_wg(&self) -> Result<()> {
        let cmd = "wg-quick";
        let fpath = make_config_dir(&self.name)?.join(format!("{}.conf", &self.name));
        let fpath_s = &fpath.display().to_string();
        std::process::Command::new(cmd)
            .args(vec!["down", fpath_s])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("Failed to remove local Wireguard interface")?;
        Ok(())
    }
    /// Generates an SSH known_hosts file, containing the automatically
    /// generated SSH hostkey for the remote server. Doing so allows
    /// us to verify the SSH connection on first use.
    fn known_hosts(&self) -> Result<String> {
        let ipv4_address = &self.server.ipv4_address()?;
        let server_host_key = &self.ssh_server_keypair.public;
        let host_line = format!("{} {}", ipv4_address, server_host_key);
        let fpath = make_config_dir(&self.name)?.join("known_hosts");
        std::fs::write(&fpath, host_line).context("Failed to create known_hosts")?;
        Ok(fpath.display().to_string())
    }
    /// Execute a shell command on the remote server.
    fn run_ssh_cmd(&self, cmd: Vec<&str>) -> Result<()> {
        tracing::trace!("Entering run_ssh_cmd");
        let ssh_kp = &self
            .ssh_client_keypair
            .write_locally(&self.name)?;
        let ssh_kp_s = ssh_kp.display().to_string();
        let known_hosts_opt = format!("UserKnownHostsFile={}", &self.known_hosts()?);
        let ipv4_address = &self.server.ipv4_address()?.to_string();
        let mut cmd_args = vec![
            "-l",
            "innisfree",
            "-i",
            &ssh_kp_s,
            "-o",
            &known_hosts_opt,
            "-o",
            "ConnectTimeout=5",
            ipv4_address,
        ];
        cmd_args.extend(cmd);
        std::process::Command::new("ssh")
            .args(cmd_args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("ssh command failed")?;
        Ok(())
    }
    /// Destroys all infrastructure, including local Wireguard interfaces,
    /// remote server, and local config dir.
    pub async fn clean(&self) -> Result<()> {
        tracing::debug!("removing local Wireguard interface");
        // Ignore errors, since we want to try all handlers
        let _ = self.bring_down_local_wg();
        let _ = self.server.destroy().await;
        clean_config_dir(&self.name)?;
        Ok(())
    }
}

/// Look up IPv4 address for remote server. Accepts a service name,
/// so that `innisfree ip` on the CLI can return an answer by inspecting
/// the on-disk config for an instance running in a separate process.
// TODO: store ip in config file locally
pub fn get_server_ip(service_name: &str) -> Result<IpAddr> {
    tracing::trace!("Looking up server IP from known_hosts file");
    let fpath = make_config_dir(service_name)?.join("known_hosts");
    let known_hosts = std::fs::read_to_string(&fpath)?;
    let host_parts: Vec<&str> = known_hosts.split(' ').collect();
    let ip: IpAddr = host_parts[0].to_string().parse()?;
    Ok(ip)
}

/// Create an interface SSH session on remote server.
pub fn open_shell(service_name: &str) -> Result<()> {
    let client_key = make_config_dir(service_name)?.join("client_id_ed25519");
    let client_key_s = client_key.display().to_string();
    let known_hosts = make_config_dir(service_name)?.join("known_hosts");
    let known_hosts_opt = format!("UserKnownHostsFile={}", known_hosts.display());
    let ipv4_address = get_server_ip(service_name)?.to_string();
    let cmd_args = vec![
        "-l",
        "innisfree",
        "-i",
        &client_key_s,
        "-o",
        &known_hosts_opt,
        "-o",
        "ConnectTimeout=5",
        &ipv4_address,
    ];
    std::process::Command::new("ssh")
        .args(cmd_args)
        .status()
        .context("SSH interactive session failed")?;
    Ok(())
}

/// Spin up local network proxy to handle passing traffic
/// between the local service(s) and the remote server.
pub async fn run_proxy(
    local_ip: IpAddr,
    dest_ip: IpAddr,
    services: Vec<ServicePort>,
) -> Result<()> {
    // We'll kick off a dedicated proxy for each service,
    // and collect the handles to await them all together, concurrently.
    let mut tasks = vec![];
    for s in services {
        let listen_addr: SocketAddr = format!("{}:{}", local_ip, &s.local_port).parse()?;
        let dest_addr: SocketAddr = format!("{}:{}", dest_ip, &s.port).parse()?;
        let h = proxy_handler(listen_addr, dest_addr);
        tasks.push(h);
    }
    // We expect the proxies to block indefinitely, except ctrl+c.
    // If they return earlier, we'll be able to inspect the errors.
    let proxy_tasks = join_all(tasks).await;
    tracing::warn!("Proxy stopped unexpectedly, no longer forwarding traffic");
    for t in proxy_tasks {
        match t {
            Ok(t) => {
                // I don't expect to see this
                tracing::debug!("Service proxy returned ok: {:?}", t);
            }
            Err(e) => {
                return Err(anyhow!("Service proxy failed: {}", e));
            }
        }
    }
    Ok(())
}
