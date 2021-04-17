"""
Handles creating the cloud node, on DigitalOcean,
in order to reserve a public IP and pass traffic
from internet back through the tunnel to LAN.
"""
import digitalocean
from pathlib import Path
import os
import time
from ruamel import yaml
import jinja2

from .ssh import SSHKeypair
from .utils import logger, CONFIG_DIR, ServicePort
from .wg import WIREGUARD_LOCAL_IP

import typing

DO_REGION = "sfo2"
DO_SIZE = "s-1vcpu-1gb"
DO_IMAGE = "ubuntu-20-04-x64"
DO_NAME = "innisfree"


class InnisfreeServer:
    def __init__(self, wg_config: str, services: typing.List[ServicePort]) -> None:
        # Prepare dynamic config vars for instance
        self.services = services
        # Generate keypairs for SSH connection
        self.ssh_client_keypair = SSHKeypair(prefix="client_")
        self.ssh_server_keypair = SSHKeypair(prefix="server_")
        self.wg_config = wg_config
        self.cloudinit_path = self.prepare_cloudinit()

        self.auth_init()
        logger.info("Creating server")
        self.droplet = self._create()
        self.name = self.droplet.name
        self.droplet_id = self.droplet.id
        logger.debug(f"Created server: {self}")

    def auth_init(self) -> None:
        api_token = os.environ["DIGITALOCEAN_API_TOKEN"]
        api_token = api_token.rstrip()
        self.api_token = api_token

    @property
    def ipv4_address(self) -> str:
        """
        Returns public IPv4 of droplet.
        """
        return str(self.droplet.ip_address)

    def __repr__(self) -> str:
        return f"<InnisfreeServer: IPv4={self.ipv4_address}>"

    def prepare_cloudinit(self) -> Path:
        """
        Loads cloudconfig template, optionally customizing with overrides.
        Plugs in dynamic SSH keypair (both client and host) info to allow
        trusted management connections.

        Returns a filepath to the cloudconfig on disk.
        """
        project_root = Path(__file__).parent.parent
        default_cloudinit_path = os.path.join(project_root, "files", "cloudinit.cfg")
        with open(default_cloudinit_path, "r") as f:
            cloudinit_config = yaml.round_trip_load(f, preserve_quotes=True)

        # Add dynamic SSH hostkeys so connection is trusted
        c = {}  # type: typing.Dict[str, typing.Any]
        c["ssh_keys"] = {}
        c["ssh_keys"]["ed25519_public"] = self.ssh_server_keypair.public
        c["ssh_keys"]["ed25519_private"] = self.ssh_server_keypair.private

        # Configure SSH user authorized_keys
        c["users"] = cloudinit_config["users"]
        c["users"][0]["ssh_authorized_keys"] = [self.ssh_client_keypair.public]

        cloudinit_config.update(c)
        cloudinit_config["write_files"].append(
            {
                "content": self.wg_config,
                "owner": "root:sudo",
                "mode": "0640",
                "path": "/tmp/innisfree.conf",
            }
        )
        cloudinit_config["write_files"].append(
            {
                "content": self.nginx_streams,
                "owner": "root:root",
                "mode": "0644",
                "path": "/etc/nginx/conf.d/stream/innisfree.conf",
            }
        )
        cloudinit_path = CONFIG_DIR.joinpath("cloudconfig")

        with open(cloudinit_path, "w") as f:
            yaml.round_trip_dump(cloudinit_config, f, default_flow_style=False, allow_unicode=True)

        return Path(cloudinit_path)

    @property
    def nginx_streams(self) -> str:
        project_root = Path(__file__).parent.parent
        nginx_template = project_root.joinpath("files", "stream.conf.j2")
        with open(nginx_template, "r") as f:
            t = jinja2.Template(f.read())
        ctx = {
            "services": self.services,
            "dest_ip": WIREGUARD_LOCAL_IP,
        }
        r = t.render(ctx)
        return r

    @property
    def user_data(self) -> str:
        fpath = self.prepare_cloudinit()
        with open(fpath, "r") as f:
            # We wrote the file, it's small
            user_data = f.read()
        return user_data

    def attach_ip(self, ip: str) -> None:
        """
        Attaches a pre-existing Floating IP to the server.
        """
        f = digitalocean.FloatingIP()
        f.ip = ip
        # Check that IP given on CLI exists
        f = f.load()
        f.assign(self.droplet.id)

    def _create(self, wait: bool = True) -> digitalocean.Droplet:
        """
        Creates DigitalOcean server for managing tunnel.
        Optionally accepts a dict of cloudinit config data,
        which will be merged as overrides to the default cloudinit template.
        """
        droplet = digitalocean.Droplet(
            token=self.api_token,
            name=DO_NAME,
            region=DO_REGION,
            image=DO_IMAGE,
            size_slug=DO_SIZE,
            user_data=self.user_data,
            backups=False,
        )
        droplet.create()

        while wait and not droplet.ip_address:
            time.sleep(5)
            droplet.load()

        return droplet
