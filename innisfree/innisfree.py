from .server import InnisfreeServer, delete_servers
from .utils import logger
import subprocess
import tempfile
import time
import socket
from pathlib import Path
from signal import signal, SIGINT


class InnisfreeManager:
    def __init__(self, proxy_address="localhost", local_port="8000", remote_port="8080") -> None:
        self.proxy_address = proxy_address
        self.local_port = local_port
        self.remote_port = remote_port

        self.server = InnisfreeServer()
        logger.info("Waiting for server to boot")
        self._wait_for_boot()
        logger.info("Server boot complete")

    @property
    def full_config(self) -> str:
        """
        Debugging method, useful for dumping info about a setup.
        """
        s = ""
        s += f"Client keypair: {self.server.client_keypair}\n"
        s += f"Server keypair: {self.server.server_keypair}\n"
        s += f"Server IPv4: {self.server.ipv4_address}\n"
        return s

    def __repr__(self) -> str:
        s = f"<InnisfreeManager: ServerIPv4={self.server.ipv4_address}>"
        return s

    @property
    def known_hosts(self) -> Path:
        _, fpath = tempfile.mkstemp()
        with open(fpath, "w") as f:
            f.write(f"{self.server.ipv4_address} {self.server.server_keypair.public}\n")
        return Path(fpath)

    def _wait_for_boot(self) -> None:
        """
        Blocks until cloud-init has finished running on the host.
        Allows for packages to be installed.
        """
        self._wait_for_ssh()
        cmd = "cloud-init status --wait"
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
        Creates an SSH tunnel to the cloud host, passing
        traffic back to the local service. Returns nothing,
        but updates the self.tunnel_process attribute with the
        subprocess.Popen value.
        """
        ssh_cmd = [
            "ssh",
            "-l",
            "innisfree",
            "-i",
            str(self.server.client_keypair.filepath),
            "-o",
            f"UserKnownHostsFile={self.known_hosts}",
            "-o",
            "ExitOnForwardFailure=yes",
            "-N",
            "-R",
            f"{self.remote_port}:{self.proxy_address}:{self.local_port}",
            self.server.ipv4_address,
        ]
        logger.debug("Opening tunnel, using command: : {}".format(" ".join(ssh_cmd)))
        self.tunnel_process = subprocess.Popen(ssh_cmd)
        if self.tunnel_process.returncode not in (None, 0):
            msg = "Failed to open tunnel"
            raise Exception(msg)

    def monitor_tunnel(self, interval=30) -> None:
        """
        Ensures that tunnel remains open. If it disconnects,
        tries to re-establish the connection. Checks every ``interval`` seconds.
        """
        time.sleep(interval)

        def handle_sigint(signal_received, frame):
            logger.info("Termination requested, cleaning up...")
            self.cleanup()

        # Exit gracefully
        signal(SIGINT, handle_sigint)

        while self.tunnel_process.poll() is None:
            logger.debug("Heartbeat: Tunnel appears healthy")
            time.sleep(interval)

        logger.error("Heartbeat: Tunnel failed, retrying")
        self.open_tunnel()
        self.monitor_tunnel()

    def close_tunnel(self) -> None:
        if not hasattr(self, "tunnel_process"):
            msg = "No tunnel has been opened"
            raise Exception(msg)
        # TODO: not convinced either terminate or kill actually results
        # in the tunnel being closed, maybe loop and poll for exit?
        # self.tunnel_process.kill()
        self.tunnel_process.terminate()

    def cleanup(self) -> None:
        """
        Tears down all created infra. Ideal for gracefully exiting.
        """
        logger.debug("Running cleanup tasks")
        logger.debug("Closing tunnel")
        self.close_tunnel()
        logger.debug("Destroy ALL innisfree servers")
        delete_servers()
        msg = "Cleanup is completed, exiting"
        raise Exception(msg)

    def run(self, cmd) -> str:
        ssh_cmd = [
            "ssh",
            "-l",
            "innisfree",
            "-i",
            str(self.server.client_keypair.filepath),
            "-o",
            f"UserKnownHostsFile={self.known_hosts}",
            self.server.ipv4_address,
        ]
        ssh_cmd += cmd.split()
        logger.debug("Running cmd: {}".format(" ".join(ssh_cmd)))
        r = subprocess.check_output(ssh_cmd).decode("utf-8")
        return r
