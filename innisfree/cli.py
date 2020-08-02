import configargparse
import sys
import requests
import os

from .utils import logger
from .innisfree import InnisfreeManager
import kopf
from .operator import create_fn


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

    logger.debug("Checking destination service")
    # TODO don't assume http; maybe assume tcp socket
    dest_service = f"http://{args.proxy_address}:{args.local_port}"
    try:
        _ = requests.head(dest_service)
        logger.debug(f"Service reachable: {dest_service}")
    except requests.exceptions.ConnectionError:
        logger.warning(f"Service unreachable: {dest_service}")

    try:
        do_api_token = os.environ["DIGITALOCEAN_API_TOKEN"]
    except KeyError:
        logger.error("DIGITALOCEAN_API_TOKEN env var not found")
        return 1

    if args.operator:
        kopf.run()

    mgr = InnisfreeManager()
    try:
        mgr.open_tunnel()
    except Exception as e:
        msg = "Failed to open tunnel: {}".format(e)
        logger.error(msg)
        return 2

    logger.info(
        f"Tunnel open: http://{mgr.server.ipv4_address} -> http://{args.proxy_address}:{args.local_port}"  # noqa
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
