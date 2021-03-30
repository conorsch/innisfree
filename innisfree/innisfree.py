from .server import InnisfreeServer, delete_servers
from .wg import WireguardManager
from .utils import logger, CONFIG_DIR, runcmd
import subprocess
import time
import socket
from pathlib import Path
from signal import signal, SIGINT


class InnisfreeManager:
    def __init__(self, ports) -> None:
        self.ports = ports
        # TODO: create firewall
        logger.info("Generating Wireguard network config")
        self.wg = WireguardManager()
        self.server = InnisfreeServer(wg_config=self.wg.wg_remote_device.config)
        logger.info("Waiting for server to boot")
        self._wait_for_boot()
        logger.info("Server boot complete")
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

    def _wait_for_ssh(self, interval=5) -> None:
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

    def monitor_tunnel(self, interval=30, retries=3) -> None:
        """
        Ensures that tunnel remains open, via ping. If any ping fails,
        script will exit non-zero. Maybe that's harsh, but would rather know.
        """
        time.sleep(interval)

        failures = 0

        def handle_sigint(signal_received, frame):
            logger.info("SIGINT, exiting gracefully...")
            self.cleanup()
            raise

        # Exit gracefully
        signal(SIGINT, handle_sigint)

        while True:
            try:
                self.test_tunnel()
                logger.debug("Heartbeat: Tunnel appears healthy")
            except subprocess.CalledProcessError:
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
        logger.dbeug("Removing local wg interface")
        self.wg.wg_local_device.down()
        delete_servers()
        # TODO: Delete firewall
        logger.info("Cleanup finished, exiting")

    def run(self, cmd) -> str:
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
        logger.debug("Running cmd: {}".format(" ".join(ssh_cmd)))
        r = subprocess.check_output(ssh_cmd).decode("utf-8")
        return r
