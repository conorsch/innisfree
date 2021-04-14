import configargparse
import sys
import os

from .utils import logger, parse_ports
from .innisfree import InnisfreeManager


INNISFREE_DEFAULT_DEST_IP = "127.0.0.1"


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
        "--dest-ip",
        action="store",
        default=INNISFREE_DEFAULT_DEST_IP,
        env_var="INNISFREE_DEST_IP",
        help="IPv4 address for proxy destination, whither traffic is forwarded",
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

    try:
        mgr = InnisfreeManager(ports=args.ports, dest_ip=args.dest_ip)
        mgr.open_tunnel()
    except Exception as e:
        logger.error(f"Failed to open tunnel: {e}")
        mgr.cleanup()
        return 2

    mgr.start_proxy()

    services = parse_ports(args.ports)
    try:
        example_get = [s for s in services if s.protocol == "TCP"][0].port
    except IndexError:
        example_get = 0

    tunnel_msg = f"Tunnel open! Proxying ports {args.ports}."
    if example_get:
        tunnel_msg += f" Try http://{mgr.server.ipv4_address}:{example_get}"
    logger.info(tunnel_msg)

    try:
        mgr.monitor_tunnel()
    except Exception as e:
        msg = "Tunnel failed unexpectedly: {}".format(e)
        logger.error(msg)
        return 3

    return 0


if __name__ == "__main__":
    sys.exit(main())
