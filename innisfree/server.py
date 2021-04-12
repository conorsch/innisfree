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

from .ssh import SSHKeypair
from .utils import logger, CONFIG_DIR

DO_REGION = "sfo2"
DO_SIZE = "s-1vcpu-1gb"
DO_IMAGE = "ubuntu-20-04-x64"
DO_NAME = "innisfree"


class InnisfreeServer:
    def __init__(self, wg_config) -> None:
        # Prepare dynamic config vars for instance
        logger.debug("Generating keypairs for connection")
        self.ssh_client_keypair = SSHKeypair(prefix="client_")
        self.ssh_server_keypair = SSHKeypair(prefix="server_")
        logger.debug("Building cloudconfig")
        self.wg_config = wg_config
        self.cloudinit_path = self.prepare_cloudinit()

        logger.debug("Initializing Digitalocean API auth")
        self.auth_init()
        logger.info("Creating server")
        self.droplet = self._create()
        self.name = self.droplet.name
        self.droplet_id = self.droplet.id
        logger.debug(f"Created server: {self}")

    def auth_init(self):
        api_token = os.environ["DIGITALOCEAN_API_TOKEN"]
        api_token = api_token.rstrip()
        self.api_token = api_token

    @property
    def ipv4_address(self):
        """
        Returns public IPv4 of droplet.
        """
        return self.droplet.ip_address

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
        c = {}  # type: dict
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
                "owner": "root:root",
                "mode": "0644",
                "path": "/tmp/innisfree.conf",
            }
        )
        cloudinit_path = CONFIG_DIR.joinpath("cloudconfig")

        with open(cloudinit_path, "w") as f:
            yaml.round_trip_dump(cloudinit_config, f, default_flow_style=False, allow_unicode=True)

        logger.debug(f"Cloudconfig filepath: {cloudinit_path}")
        return Path(cloudinit_path)

    @property
    def user_data(self) -> str:
        fpath = self.prepare_cloudinit()
        with open(fpath, "r") as f:
            # We wrote the file, it's small
            user_data = f.read()
        return user_data

    def _create(self, wait=True) -> digitalocean.Droplet:
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
        logger.debug("Creating droplet via python-digitalocean...")
        droplet.create()

        while wait and not droplet.ip_address:
            time.sleep(5)
            droplet.load()

        return droplet


def delete_servers() -> None:
    do_token = os.environ["DIGITALOCEAN_API_TOKEN"]
    mgr = digitalocean.Manager(token=do_token)
    all_droplets = mgr.get_all_droplets()

    innisfree_droplets = [d for d in all_droplets if d.name == "innisfree"]
    for d in innisfree_droplets:
        d.delete()
