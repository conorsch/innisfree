import socket
import select

from .utils import logger
from .wg import WIREGUARD_LOCAL_IP

PROXY_LOCAL_ADDR = WIREGUARD_LOCAL_IP
PROXY_LOCAL_PORT = 8888

PROXY_REMOTE_ADDR = "127.0.0.1"
PROXY_REMOTE_PORT = 8000

CHUNK_SIZE = 1024


def server_loop(
    local_addr: str = PROXY_LOCAL_ADDR,
    local_port: int = PROXY_LOCAL_PORT,
    remote_addr: str = PROXY_REMOTE_ADDR,
    remote_port: int = PROXY_REMOTE_PORT,
) -> None:
    # TODO: Add UDP support, in another function
    # Bind on inbound wireguard device
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        server.bind((local_addr, local_port))
    # OSError: [Errno 98] Address already in use
    except OSError as e:
        logger.error(f"Failed to bind to {local_addr}:{local_port}, {e}")
        raise

    logger.info(f"Listening on {local_addr}:{local_port}")
    server.listen(10)

    # This code, MIT from 2016, was quite helpful:
    # https://github.com/rsc-dev/pyproxy/blob/master/code/pyproxy.py

    while True:
        # Answer the door
        s_src, _ = server.accept()
        s_dst = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            s_dst.connect((remote_addr, remote_port))
        except ConnectionRefusedError as e:
            logger.debug(f"Failed to reach {remote_addr}:{remote_port}, {e}")
            pass

        # Store sockets for selection, so we can handle multiple clients
        sockets = [s_src, s_dst]

        while True:
            # Check what's ready for reading. Socket may have been closed,
            # so exit loop if nothing's replying.
            try:
                s_read, _, _ = select.select(sockets, [], [])
            # e.g. ValueError: file descriptor cannot be a negative integer (-1)
            except ValueError:
                break

            # Compare socket direction, support two-way conversation
            for s in s_read:
                # Sometimes this can raise
                # ConnectionResetError: [Errno 104] Connection reset by peer
                d = s.recv(CHUNK_SIZE)
                if s == s_src:
                    s_dst.send(d)
                elif s == s_dst:
                    s_src.send(d)
                else:
                    t = type(s)
                    logger.debug(f"Found unknown socket type: {t} {s}")
                if not d:
                    s_src.close()
                    break


if __name__ == "__main__":
    server_loop()
