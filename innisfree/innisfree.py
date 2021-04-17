from .server import InnisfreeServer
from .wg import WireguardManager
from .utils import logger, CONFIG_DIR, runcmd, parse_ports, clean_config_dir
from subprocess import check_output, check_call, PIPE, CalledProcessError, Popen
import time
import socket
import threading
from pathlib import Path
from signal import signal, SIGINT
from .wg import WIREGUARD_LOCAL_IP
from .proxy import server_loop


class InnisfreeManager:
    def __init__(self, ports: str, dest_ip: str = "127.0.0.1", floating_ip: str = "") -> None:
        self.ports = parse_ports(ports)
        # TODO: create firewall
        self.dest_ip = dest_ip
        self.floating_ip = floating_ip

    def up(self) -> None:
        self.wg = WireguardManager()
        self.server = InnisfreeServer(
            wg_config=self.wg.wg_remote_device.config, services=self.ports
        )
        logger.debug("Waiting for server to boot")
        self._wait_for_boot()
        logger.debug("Server boot complete")
        if self.floating_ip:
            logger.debug("Attaching floating IP")
            self.server.attach_ip(self.floating_ip)
        logger.debug("Updating remote endpoint")
        self.wg.wg_remote_host.endpoint = self.server.ipv4_address
        logger.debug("Bringing up remote wg iface")
        self.run("wg-quick up /tmp/innisfree.conf")
        logger.info("Bringing up local wg iface")
        self.wg.wg_local_device.up()
        logger.debug("Testing tunnel for connectivity...")
        self.test_tunnel()

    @property
    def full_config(self) -> str:
        """
        Debugging method, useful for dumping info about a setup.
        """
        s = ""
        s += f"SSH keypair: {self.server.ssh_server_keypair}\n"
        # s += f"Wireguard keypair: {self.server.wireguard_keypair}\n"
        s += f"Server IPv4: {self.server.ipv4_address}\n"
        return s

    def __repr__(self) -> str:
        s = f"<InnisfreeManager: ServerIPv4={self.server.ipv4_address}>"
        return s

    def start_proxy(self) -> None:
        """
        Creates a new thread for handling traffic to the local service being exposed.
        """
        for s in self.ports:
            proxy_thread = threading.Thread(
                target=server_loop, args=(WIREGUARD_LOCAL_IP, s.port, self.dest_ip, s.port),
            )
            proxy_thread.start()
            logger.debug(f"Starting thread for service {s}")

    @property
    def known_hosts(self) -> Path:
        fpath = CONFIG_DIR.joinpath("known_hosts")
        with open(fpath, "w") as f:
            f.write(f"{self.server.ipv4_address} {self.server.ssh_server_keypair.public}\n")
        return fpath

    def _wait_for_boot(self) -> None:
        """
        Blocks until cloud-init has finished running on the host.
        Allows for packages to be installed.
        """
        self._wait_for_ssh()
        cmd = "cloud-init status --long --wait"
        self.run(cmd)

    def _wait_for_ssh(self, interval: int = 5) -> None:
        """
        Blocks until a TCP:22 is open. Does not validate
        a successful auth connection. Checks every ``interval`` seconds.
        """
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        connected = False
        while not connected:
            try:
                s.connect((self.server.ipv4_address, 22))
                connected = True
                logger.debug("SSH port on remote host is open, proceeding")
            except socket.error:
                logger.debug("SSH port on remote host closed, waiting...")
                time.sleep(interval)

        # Sleep a bit more, since SSH just opened up
        time.sleep(interval)

    def open_tunnel(self) -> None:
        """
        The tunnel is stateless, so we're not really opening it, we'll
        just test that it is indeed open, via ping.
        """
        logger.debug("Trying to open tunnel, nothing to open (wireguard)")

    def test_tunnel(self) -> None:
        """
        Send a ping from local wg iface to remote wg iface.
        """
        cmd = f"ping -c1 {self.wg.wg_remote_host.address}".split()
        runcmd(cmd)

    def monitor_tunnel(self, interval: int = 300, retries: int = 3) -> None:
        """
        Ensures that tunnel remains open, via ping. If any ping fails,
        script will exit non-zero. Maybe that's harsh, but would rather know.
        """
        time.sleep(interval)

        failures = 0

        def handle_sigint(signal_received, frame) -> None:
            logger.info("SIGINT, exiting gracefully...")
            self.cleanup()
            raise Exception("Exiting gracefully")

        # Exit gracefully
        signal(SIGINT, handle_sigint)

        while True:
            try:
                self.test_tunnel()
                logger.debug("Heartbeat: Tunnel appears healthy")
            except CalledProcessError:
                logger.error("Heartbeat: Tunnel failed!")
                failures += 1
                if failures < retries:
                    continue
                raise
            time.sleep(interval)

    def cleanup(self) -> None:
        """
        Tears down all created infra. Ideal for gracefully exiting.
        """
        logger.debug("Removing local wg interface")
        self.wg.wg_local_device.down()
        logger.debug("Destroying droplet")
        self.server.droplet.destroy()
        clean_config_dir()
        # TODO: Delete firewall
        logger.info("Cleanup finished, exiting")

    @classmethod
    def get_server_ip(self) -> str:
        """
        Looks up IPv4 address of server and returns it.
        Assumes already created, should handle more gracefully if not.
        """
        with open(CONFIG_DIR.joinpath("known_hosts"), "r") as f:
            server_ip = f.read().split()[0]
        return server_ip

    @classmethod
    def open_shell(self) -> None:
        """
        Start interactive SSH shell, for logging into cloud node.
        Makes some assumptions about local config in order to log in
        via a separate process from the already running innisfree tunnel process.
        """
        client_key = CONFIG_DIR.joinpath("client_id_ed25519")
        known_hosts = CONFIG_DIR.joinpath("known_hosts")
        server_ip = self.get_server_ip()
        ssh_cmd = [
            "ssh",
            "-l",
            "innisfree",
            "-i",
            client_key,
            "-o",
            f"UserKnownHostsFile={known_hosts}",
            server_ip,
        ]
        p = Popen(ssh_cmd)
        p.communicate()

    def run(self, cmd: str, quiet: bool = False) -> str:
        ssh_cmd = [
            "ssh",
            "-l",
            "innisfree",
            "-i",
            str(self.server.ssh_client_keypair.filepath),
            "-o",
            f"UserKnownHostsFile={self.known_hosts}",
            self.server.ipv4_address,
        ]
        ssh_cmd += cmd.split()
        logger.debug("running cmd: {}".format(" ".join(ssh_cmd)))
        if quiet:
            check_call(ssh_cmd, stdout=PIPE, stdin=PIPE, stderr=PIPE)
            r = ""
        else:
            r = check_output(ssh_cmd, stdin=PIPE, stderr=PIPE).decode("utf-8")
        return r
