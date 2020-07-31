# type: ignore
from innisfree import __version__, InnisfreeManager
import pytest
import subprocess
import requests

from innisfree.server import delete_servers


def test_version():
    assert __version__ == "0.1.0"


def setup_module(module):
    test_dir = "/tmp/innisfree-test-public"
    subprocess.check_call(["mkdir", "-p", test_dir])
    with open(test_dir + "/index.html", "w") as f:
        f.write("Hello, world!")

    p = subprocess.Popen(
        ["python3", "-m", "http.server", "--bind", "localhost", "--directory", test_dir]
    )
    assert p.returncode in (None, 0)


@pytest.fixture(scope="class")
def innisfree_manager():
    delete_servers()
    mgr = InnisfreeManager()
    yield mgr
    delete_servers()


class TestInnisfreeManager:
    def test_hostkey_exists(self, innisfree_manager):
        scan_cmd = f"ssh-keyscan -t ed25519 {innisfree_manager.server.ipv4_address}"
        raw_result = subprocess.check_output(scan_cmd.split())
        # Output is two lines, first is a comment, second is hostkey.
        result = raw_result.decode("utf-8").rstrip().split("\n")[-1]
        with open(innisfree_manager.known_hosts) as f:
            known_hosts = f.read().rstrip()
        assert result == known_hosts

    def test_connect_to_server(self, innisfree_manager):
        innisfree_manager.run("uptime")

    def test_open_tunnel(self, innisfree_manager):
        innisfree_manager.open_tunnel()

    def test_http_get(self, innisfree_manager):
        r = requests.get(f"http://{innisfree_manager.server.ipv4_address}")
        assert r.ok
        assert r.content == b"Hello, world!"

    def test_close_tunnel(self, innisfree_manager):
        innisfree_manager.close_tunnel()
