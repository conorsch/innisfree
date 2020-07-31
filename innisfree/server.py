"""
Handles creating the cloud node, on DigitalOcean,
in order to reserve a public IP and pass traffic
from internet back through the tunnel to LAN.
"""
import subprocess
from pathlib import Path
import os
import time
import json
import tempfile
from ruamel import yaml
from typing import Dict

from .ssh import SSHKeypair
from .utils import logger

DO_REGION = "sfo2"
DO_SIZE = "s-2vcpu-2gb"
DO_IMAGE = "debian-10-x64"
DO_NAME = "innisfree"


class InnisfreeServer:
    def __init__(self) -> None:
        # Prepare dynamic config vars for instance
        logger.debug("Generating keypairs for connection")
        self.client_keypair = SSHKeypair()
        self.server_keypair = SSHKeypair()
        logger.debug("Building cloudconfig")
        self.cloudinit_path = self.prepare_cloudinit()

        logger.info("Creating server")
        self.json_config = self._create()
        self.name = self.json_config["name"]
        self.droplet_id = self.json_config["id"]
        self.ipv4_address = self.json_config["networks"]["v4"][0]["ip_address"]

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
        c["ssh_keys"]["ed25519_public"] = self.server_keypair.public
        c["ssh_keys"]["ed25519_private"] = self.server_keypair.private

        # Configure SSH user authorized_keys
        c["users"] = cloudinit_config["users"]
        c["users"][0]["ssh_authorized_keys"] = [self.client_keypair.public]

        cloudinit_config.update(c)

        _, cloudinit_path = tempfile.mkstemp()

        with open(cloudinit_path, "w") as f:
            yaml.round_trip_dump(cloudinit_config, f, default_flow_style=False, allow_unicode=True)

        logger.debug(f"Cloudconfig filepath: {cloudinit_path}")
        return Path(cloudinit_path)

    def _create(self) -> Dict:
        """
        Creates DigitalOcean server for managing tunnel.
        Optionally accepts a dict of cloudinit config data,
        which will be merged as overrides to the default cloudinit template.
        """
        cmd = [
            "doctl",
            "compute",
            "droplet",
            "create",
            "--region",
            DO_REGION,
            "--image",
            DO_IMAGE,
            "--size",
            DO_SIZE,
            "--user-data-file",
            str(self.cloudinit_path),
            "--wait",
            "--output",
            "json",
            DO_NAME,
        ]
        logger.debug("Droplet creation cmd: {}".format(" ".join(cmd)))  # type: ignore
        raw_output = subprocess.check_output(cmd).decode("utf-8").rstrip()  # type: ignore
        server_json = json.loads(raw_output)[0]
        return server_json


def delete_servers() -> None:
    cmd = "doctl compute droplet list innisfree --format ID --no-header".split()
    raw_ids = ""
    try:
        raw_ids = subprocess.check_output(cmd).decode("utf-8").rstrip()
    except subprocess.CalledProcessError:
        pass
    ids = raw_ids.split("\n")
    ids = [x for x in ids if x != ""]

    for x in ids:
        cmd = "doctl compute droplet delete -f {}".format(x).split()
        subprocess.check_call(cmd)
        time.sleep(5)
