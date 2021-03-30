import configargparse
import sys
import os

from .utils import logger
from .innisfree import InnisfreeManager


def parse_args():
    parser = configargparse.ArgumentParser()
    parser.add_argument(
        "--ports",
        action="store",
        default="80/TCP,443/TCP",
        env_var="INNISFREE_PORTS",
        help="List of service ports to forward, comma-separated",
    )
    parser.add_argument(
        "--operator",
        action="store_true",
        default=False,
        env_var="INNISFREE_OPERATOR",
        help="Run in operator mode, suitable for inside k8s cluster",
    )
    args = parser.parse_args()
    return args


def main() -> int:
    logger.debug("Parsing CLI args")
    args = parse_args()

    if "DIGITALOCEAN_API_TOKEN" not in os.environ:
        logger.error("DIGITALOCEAN_API_TOKEN env var not found")
        return 1

    mgr = InnisfreeManager(args.ports)
    try:
        mgr.open_tunnel()
    except Exception as e:
        msg = "Failed to open tunnel: {}".format(e)
        logger.error(msg)
        return 2

    logger.info(
        f"Tunnel open! Proxying ports {args.ports}. Try http://{mgr.server.ipv4_address}:8080 "  # noqa
    )

    try:
        mgr.monitor_tunnel()
    except Exception as e:
        msg = "Tunnel failed unexpectedly: {}".format(e)
        logger.error(msg)
        return 3

    return 0


if __name__ == "__main__":
    sys.exit(main())
