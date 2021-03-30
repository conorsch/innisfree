# type: ignore
from innisfree.utils import parse_ports


def test_ports_web():
    port_spec = "80/TCP,443/TCP"
    ports = parse_ports(port_spec)
    assert len(ports) == 2
    for p in ports:
        assert p.protocol == "TCP"

    assert len(list(filter(lambda x: x.port == "80", ports))) == 1
    assert len(list(filter(lambda x: x.port == "443", ports))) == 1


def test_ports_http_only():
    port_spec = "80"
    ports = parse_ports(port_spec)
    assert len(ports) == 1
    for p in ports:
        assert p.protocol == "TCP"

    assert len(list(filter(lambda x: x.port == "80", ports))) == 1
    assert len(list(filter(lambda x: x.port == "443", ports))) == 0


def test_ports_udp():
    port_spec = "80,443,4443/TCP,4000/UDP"
    ports = parse_ports(port_spec)
    assert len(ports) == 4
    assert len(list(filter(lambda x: x.port == "80" and x.protocol == "TCP", ports))) == 1
    assert len(list(filter(lambda x: x.port == "443" and x.protocol == "TCP", ports))) == 1
    assert len(list(filter(lambda x: x.port == "4443" and x.protocol == "TCP", ports))) == 1
    assert len(list(filter(lambda x: x.port == "4000" and x.protocol == "UDP", ports))) == 1
