import logging
import os
from subprocess import check_call, PIPE
from pathlib import Path


def get_logger():

    logging.basicConfig(
        format="%(asctime)s %(levelname)-8s %(message)s",
        level=logging.DEBUG,
        datefmt="%Y-%m-%d %H:%M:%S",
    )

    logger = logging.getLogger("innisfree")
    return logger


logger = get_logger()


class ServicePort:
    def __init__(self, spec):
        if "/" in spec:
            port, protocol = spec.split("/")
        else:
            port = spec
            protocol = "TCP"
        self.port = port
        self.protocol = protocol


def parse_ports(raw_spec):
    """
    Handles a comma-separated list of service ports, with protocols optionally
    appended (assumes TCP).
    """
    raw_ports = raw_spec.split(",")
    return [ServicePort(p) for p in raw_ports]


def make_config_dir():
    config_dir = Path("~/.config/innisfree").expanduser()
    os.makedirs(config_dir, mode=0o750, exist_ok=True)
    return config_dir


def runcmd(cmd):
    check_call(cmd, stdout=PIPE, stdin=PIPE, stderr=PIPE)


CONFIG_DIR = make_config_dir()
