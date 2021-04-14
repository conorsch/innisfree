import logging
import os
from subprocess import check_call, PIPE
from pathlib import Path
from typing import List


def get_logger() -> logging.Logger:

    logging.basicConfig(
        format="%(asctime)s %(levelname)-8s %(message)s",
        level=logging.DEBUG,
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    logger = logging.getLogger("innisfree")
    return logger


logger = get_logger()


class ServicePort(object):
    def __init__(self, spec: str) -> None:
        if "/" in spec:
            port, protocol = spec.split("/")
        else:
            port = spec
            protocol = "TCP"
        self.port = int(port)
        self.protocol = protocol
        if self.protocol.upper() not in ["TCP", "UDP"]:
            raise ValueError(f"Protocol must be 'TCP' or 'UDP', received {self.protocol}")

    def __repr__(self) -> str:
        return f"<ServicePort {self.port}/{self.protocol}"


def parse_ports(raw_spec: str) -> List[ServicePort]:
    """
    Handles a comma-separated list of service ports, with protocols optionally
    appended (assumes TCP).
    """
    raw_ports = raw_spec.split(",")
    return [ServicePort(p) for p in raw_ports]


def make_config_dir() -> Path:
    config_dir = Path("~/.config/innisfree").expanduser()
    os.makedirs(config_dir, mode=0o750, exist_ok=True)
    return config_dir


def clean_config_dir() -> None:
    config_dir = make_config_dir()
    for f in config_dir.glob("*"):
        f.unlink()


def runcmd(cmd: List[str]) -> None:
    check_call(cmd, stdout=PIPE, stdin=PIPE, stderr=PIPE)


CONFIG_DIR = make_config_dir()
