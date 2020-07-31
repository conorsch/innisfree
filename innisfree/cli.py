import configargparse
import sys
import requests

from .utils import logger
from .innisfree import InnisfreeManager


def parse_args():
    parser = configargparse.ArgumentParser()
    parser.add_argument(
        "--remote-port",
        action="store",
        default="80",
        env_var="INNISFREE_REMOTE_PORT",
        help="Port on public IP to listen on",
    )
    parser.add_argument(
        "--local-port",
        action="store",
        default="8000",
        env_var="INNISFREE_LOCAL_PORT",
        help="Port of local service to expose",
    )
    parser.add_argument(
        "--proxy-address",
        action="store",
        default="localhost",
        env_var="INNISFREE_PROXY_ADDRESS",
        help="Destination IP for forwarded traffic",
    )
    args = parser.parse_args()
    return args


def main() -> int:
    logger.debug("Parsing CLI args")
    args = parse_args()

    logger.debug("Checking destination service")
    # TODO don't assume http; maybe assume tcp socket
    dest_service = f"http://{args.proxy_address}:{args.local_port}"
    r = requests.head(dest_service)
    if not r.ok:
        logger.warn("Service unreachable: {dest_service}")
    else:
        logger.debug("Service reachable: {dest_service}")

    mgr = InnisfreeManager()
    try:
        mgr.open_tunnel()
    except Exception as e:
        msg = "Failed to open tunnel: {}".format(e)
        logger.error(msg)
        return 1
    logger.info(
        f"Tunnel open: http://{mgr.server.ipv4_address} -> http://{args.proxy_address}:{args.local_port}"  # noqa
    )
    try:
        mgr.monitor_tunnel()
    except Exception as e:
        msg = "Tunnel failed unexpectedly: {}".format(e)
        logger.error(msg)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
