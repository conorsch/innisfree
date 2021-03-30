import subprocess
import os
from ruamel.yaml.scalarstring import PreservedScalarString as pss
from typing import Tuple
from pathlib import Path
from .utils import CONFIG_DIR


class SSHKeypair:
    def __init__(self, prefix="") -> None:
        self.prefix = prefix
        (privkey_filepath, privkey, pubkey) = self._create()
        self.filepath = privkey_filepath
        self.private = privkey
        self.public = pubkey

    def _create(self) -> Tuple[Path, str, str]:
        """
        Generate an ED25519 SSH keypair, returning an obj with .private & .public.
        Uses ssh-keygen to generate, since SSH uses a special format. More research
        required to use e.g. cryptography to generate in Python without shelling out.
        """
        privkey_filepath = CONFIG_DIR.joinpath(f"{self.prefix}id_ed25519")
        pubkey_filepath = Path(str(privkey_filepath) + ".pub")
        # Deleting because ssh-keygen can't clobber files
        if os.path.exists(privkey_filepath):
            os.unlink(privkey_filepath)
        if os.path.exists(pubkey_filepath):
            os.unlink(pubkey_filepath)

        cmd = [
            "ssh-keygen",
            "-t",
            "ed25519",
            "-P",
            "",
            "-f",
            str(privkey_filepath),
            "-C",
            "",
            "-q",
        ]
        subprocess.check_call(cmd)
        with open(privkey_filepath, "r") as f:
            privkey = pss(f.read())

        with open(pubkey_filepath, "r") as f:
            pubkey = f.read().rstrip()

        return (privkey_filepath, privkey, pubkey)

    def __repr__(self) -> str:
        s = f"<SSHKeypair pubkey: {self.public}, filepath: {self.filepath}"
        return s
