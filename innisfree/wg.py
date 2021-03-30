import subprocess
import shutil
import jinja2
from typing import List
from subprocess import call, Popen, PIPE
from typing import Tuple
from pathlib import Path

from .utils import CONFIG_DIR, runcmd

WIREGUARD_LISTEN_PORT = "51820"


class WireguardKeypair:
    def __init__(self) -> None:
        if shutil.which("wg") is None:
            raise WireguardNotFound()
        (privkey, pubkey) = self._create()
        self.private = privkey
        self.public = pubkey

    def _create(self) -> Tuple[str, str]:
        """
        Generate a Wireguard keypair, returning an obj with .private & .public.
        Uses 'wg genkey' generate. More research
        required to use e.g. cryptography to generate in Python without shelling out.
        """
        cmd = ["wg", "genkey"]
        privkey_output = subprocess.check_output(cmd)

        p = Popen(["wg", "pubkey"], stdout=PIPE, stdin=PIPE, stderr=PIPE)
        pubkey_output = p.communicate(input=privkey_output)[0]

        privkey = privkey_output.decode("utf-8").rstrip()
        pubkey = pubkey_output.decode("utf-8").rstrip()

        return (privkey, pubkey)

    def __repr__(self) -> str:
        s = f"<WireguardKeypair pubkey: {self.public}"
        return s


class WireguardNotFound(Exception):
    pass


class WireguardHost:
    """
    Represents a single host on a Wireguard network. For the purposes
    of Innisfree, a subnet of /30 is assumed: we only need two (2) hosts,
    the local service that proxies, and the remote endpoint for ingress.
    """

    def __init__(self, name: str, address: str, endpoint: str, listenport: str) -> None:
        self.name = name
        self.address = address
        self.endpoint = endpoint
        self.listenport = listenport
        kp = WireguardKeypair()
        self.privatekey = kp.private
        self.publickey = kp.public


class WireguardDevice:
    """
    Represents a network interface for Wireguard. Requires config info
    for *all* Wireguard hosts on the Wireguard network CIDR, and also
    a `name`, to distinguish which hosts are peers.
    """

    def __init__(self, name: str, hosts: List[WireguardHost]) -> None:
        self.name = name
        self.hosts = hosts
        self.config_filepath = CONFIG_DIR.joinpath("innisfree.conf")

    @property
    def config(self):
        """
        Generates device config from host info, returns string contents
        of a valid Wireguard config file for a device.
        """
        project_root = Path(__file__).parent.parent
        wg_template = project_root.joinpath("files", "wg0.conf.j2")
        with open(wg_template, "r") as f:
            t = jinja2.Template(f.read())
        ctx = {
            "wireguard_hosts": self.hosts,
            "wireguard_name": self.name,
        }
        r = t.render(ctx)
        return r

    def create(self):
        with open(self.config_filepath, "w") as f:
            f.write(self.config)

    def down(self):
        cmd = f"wg-quick down {self.config_filepath}".split()
        # No error-checking because it may not exist yet
        call(cmd, stdout=PIPE, stdin=PIPE, stderr=PIPE)

    def up(self):
        self.create()
        # It's possible the iface exists already, take it down
        self.down()
        cmd = f"wg-quick up {self.config_filepath}".split()
        runcmd(cmd)


class WireguardManager:
    """
    Handles munging the various wg classes. Requires no arguments.
    Ideally we'd pass in the IPv4 of the remote endpoint here, but
    we won't know it during cloudinit. We'll update the local dev
    with the endpoint after both devs are bootstrapped.
    """

    def __init__(self):
        # TODO: make CIDR customizable
        self.wg_local_ip = "10.50.0.1"
        self.wg_local_name = "innisfree_local"
        self.wg_local_host = WireguardHost(
            name=self.wg_local_name, address=self.wg_local_ip, endpoint="", listenport="",
        )
        self.wg_remote_ip = "10.50.0.2"
        self.wg_remote_name = "innisfree_remote"
        self.wg_remote_host = WireguardHost(
            name=self.wg_remote_name,
            address=self.wg_remote_ip,
            endpoint="",
            listenport=WIREGUARD_LISTEN_PORT,
        )
        self.hosts = [self.wg_local_host, self.wg_remote_host]
        self.wg_local_device = WireguardDevice(name=self.wg_local_name, hosts=self.hosts)
        self.wg_remote_device = WireguardDevice(name=self.wg_remote_name, hosts=self.hosts)
