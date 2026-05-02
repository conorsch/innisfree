//! In-process Wireguard runtime backed by [`boringtun`].
//!
//! Supersedes the previous `wg-quick up` / `wg-quick down` shellouts:
//! we own a [`DeviceHandle`] for the lifetime of the tunnel, configure
//! it through the in-process UAPI socket via a [`UnixStream`] pair, and
//! use [`rtnetlink`] to assign the interface address and bring it up.

use anyhow::{anyhow, Context, Result};
use boringtun::device::{DeviceConfig, DeviceHandle};
use futures::stream::TryStreamExt;
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::os::fd::IntoRawFd;
use std::os::unix::net::UnixStream;

use crate::wg::WireguardDevice;

/// Wireguard's recommended MTU on top of an Ethernet link (1500 - 80
/// for IPv6 + UDP + Wireguard overhead). Same value `wg-quick` uses.
const WG_MTU: u32 = 1420;

/// Local Wireguard subnet width — see [`crate::net`].
const WG_INTERFACE_PREFIX: u8 = 30;

/// `IFNAMSIZ - 1` on Linux. Interface names longer than this are
/// rejected by the kernel.
const IFNAMSIZ: usize = 15;

/// Live local end of an Innisfree Wireguard tunnel.
///
/// While this value is held, boringtun's worker threads are running and
/// the TUN interface is up. Dropping it tears the interface down: the
/// `DeviceHandle` `Drop` impl signals exit, and closing our end of the
/// UAPI socket pair causes boringtun to release the kernel resources.
pub struct LocalWg {
    iface_name: String,
    _handle: DeviceHandle,
    _uapi_socket: UnixStream,
}

impl LocalWg {
    /// Bring the local Wireguard interface up.
    ///
    /// `device.interface` describes our local end (private key, address);
    /// `device.peer` describes the remote endpoint we're pairing with.
    /// The peer must have `endpoint` populated — we are the initiator.
    pub async fn start(device: &WireguardDevice) -> Result<LocalWg> {
        let iface_name = sanitize_iface_name(&device.name)?;
        let uapi_payload = build_uapi_set(device)?;

        let (boring_end, mut our_end) =
            UnixStream::pair().context("failed to create UAPI socketpair")?;

        let config = DeviceConfig {
            n_threads: 2,
            use_connected_socket: true,
            use_multi_queue: false,
            uapi_fd: boring_end.into_raw_fd(),
        };

        // boringtun's `DeviceHandle::new` is synchronous: it opens
        // /dev/net/tun, binds UDP sockets, and spawns its own worker
        // threads. Run on the blocking pool to avoid stalling the
        // tokio reactor.
        let iface_for_spawn = iface_name.clone();
        let handle = tokio::task::spawn_blocking(move || {
            DeviceHandle::new(&iface_for_spawn, config)
                .map_err(|e| anyhow!("boringtun DeviceHandle::new failed: {e:?}"))
        })
        .await
        .context("blocking task panicked while creating boringtun device")??;

        write_uapi(&mut our_end, &uapi_payload)
            .context("failed to configure boringtun via UAPI")?;

        configure_iface(
            &iface_name,
            device.interface.address,
            WG_INTERFACE_PREFIX,
            WG_MTU,
        )
        .await
        .context("failed to configure local wireguard interface")?;

        tracing::info!(
            iface = %iface_name,
            address = %device.interface.address,
            "local wireguard interface ready"
        );

        Ok(LocalWg {
            iface_name,
            _handle: handle,
            _uapi_socket: our_end,
        })
    }
}

impl Drop for LocalWg {
    fn drop(&mut self) {
        tracing::debug!(iface = %self.iface_name, "tearing down local wireguard interface");
    }
}

/// Reject interface names too long for the kernel rather than
/// silently truncate — silent truncation makes name collisions
/// between two services indistinguishable.
fn sanitize_iface_name(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Err(anyhow!("interface name is empty"));
    }
    if raw.len() > IFNAMSIZ {
        return Err(anyhow!(
            "interface name '{raw}' exceeds Linux IFNAMSIZ ({IFNAMSIZ}); pick a shorter --name"
        ));
    }
    Ok(raw.to_string())
}

/// Render a Wireguard UAPI `set=1` block for the local end of the tunnel.
///
/// Format reference: <https://www.wireguard.com/xplatform/#configuration-protocol>.
/// Keys travel as 64-char lowercase hex (not the base64 we hold them as).
fn build_uapi_set(device: &WireguardDevice) -> Result<String> {
    let priv_hex = hex_encode(&device.interface.keypair.private_bytes()?);
    let peer_pub_hex = hex_encode(&device.peer.keypair.public_bytes()?);

    let mut payload = String::with_capacity(512);
    payload.push_str("set=1\n");
    payload.push_str(&format!("private_key={}\n", priv_hex));
    // Omit listen_port: the local end always initiates, so a kernel-
    // assigned ephemeral port is fine.
    payload.push_str("replace_peers=true\n");
    payload.push_str(&format!("public_key={}\n", peer_pub_hex));
    if let Some(endpoint) = device.peer.endpoint {
        payload.push_str(&format!(
            "endpoint={}:{}\n",
            endpoint, device.peer.listenport
        ));
    }
    payload.push_str("persistent_keepalive_interval=25\n");
    payload.push_str("replace_allowed_ips=true\n");
    payload.push_str(&format!("allowed_ip={}/32\n", device.peer.address));
    payload.push('\n');
    Ok(payload)
}

fn write_uapi(stream: &mut UnixStream, payload: &str) -> Result<()> {
    stream.write_all(payload.as_bytes())?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let mut errno: Option<i32> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).context("UAPI read failed")?;
        if n == 0 {
            return Err(anyhow!("UAPI socket closed before reply"));
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(rest) = line.strip_prefix("errno=") {
            errno = Some(rest.parse().context("malformed UAPI errno")?);
        }
    }
    match errno {
        Some(0) => Ok(()),
        Some(e) => Err(anyhow!("UAPI reported errno={e}")),
        None => Err(anyhow!("UAPI reply contained no errno")),
    }
}

async fn configure_iface(name: &str, address: IpAddr, prefix: u8, mtu: u32) -> Result<()> {
    let (connection, handle, _) =
        rtnetlink::new_connection().context("failed to open netlink connection")?;
    let conn_task = tokio::spawn(connection);

    let result = async {
        let mut links = handle.link().get().match_name(name.to_string()).execute();
        let link = links
            .try_next()
            .await
            .context("netlink: link lookup failed")?
            .ok_or_else(|| anyhow!("interface {name} not found after boringtun spawn"))?;
        let ifindex = link.header.index;

        handle
            .address()
            .add(ifindex, address, prefix)
            .execute()
            .await
            .context("netlink: failed to assign address")?;

        handle
            .link()
            .set(ifindex)
            .mtu(mtu)
            .up()
            .execute()
            .await
            .context("netlink: failed to bring link up")?;

        Ok::<_, anyhow::Error>(())
    }
    .await;

    conn_task.abort();
    result
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wg::{WireguardDevice, WireguardHost, WireguardKeypair};

    fn fake_device() -> Result<WireguardDevice> {
        let local = WireguardHost {
            name: "local".into(),
            address: "10.50.0.1".parse()?,
            endpoint: None,
            listenport: 0,
            keypair: WireguardKeypair::new()?,
        };
        let peer = WireguardHost {
            name: "remote".into(),
            address: "10.50.0.2".parse()?,
            endpoint: Some("203.0.113.10".parse()?),
            listenport: 51820,
            keypair: WireguardKeypair::new()?,
        };
        Ok(WireguardDevice {
            name: "innisfree-foo".into(),
            interface: local,
            peer,
        })
    }

    #[test]
    fn iface_name_within_limit_passes() -> Result<()> {
        assert_eq!(sanitize_iface_name("innisfree-foo")?, "innisfree-foo");
        Ok(())
    }

    #[test]
    fn iface_name_too_long_errors() {
        let err = sanitize_iface_name("innisfree-thirty-chars-here").unwrap_err();
        assert!(err.to_string().contains("IFNAMSIZ"));
    }

    #[test]
    fn iface_name_empty_errors() {
        assert!(sanitize_iface_name("").is_err());
    }

    #[test]
    fn uapi_set_includes_required_lines() -> Result<()> {
        let dev = fake_device()?;
        let payload = build_uapi_set(&dev)?;
        assert!(payload.starts_with("set=1\n"));
        assert!(payload.ends_with("\n\n"), "must terminate with blank line");
        assert!(payload.contains("private_key="));
        assert!(payload.contains("public_key="));
        assert!(payload.contains("endpoint=203.0.113.10:51820"));
        assert!(payload.contains("allowed_ip=10.50.0.2/32"));
        assert!(payload.contains("persistent_keepalive_interval=25"));
        assert!(payload.contains("replace_peers=true"));
        assert!(payload.contains("replace_allowed_ips=true"));
        // No base64 leakage — UAPI keys are hex.
        assert!(!payload.contains(&dev.interface.keypair.private));
        assert!(!payload.contains(&dev.peer.keypair.public));
        Ok(())
    }

    #[test]
    fn uapi_set_keys_are_hex() -> Result<()> {
        let dev = fake_device()?;
        let payload = build_uapi_set(&dev)?;
        for line in payload.lines() {
            if let Some(hex) = line.strip_prefix("private_key=") {
                assert_eq!(hex.len(), 64);
                assert!(hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
            }
            if let Some(hex) = line.strip_prefix("public_key=") {
                assert_eq!(hex.len(), 64);
                assert!(hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
            }
        }
        Ok(())
    }

    #[test]
    fn uapi_omits_endpoint_when_none() -> Result<()> {
        let mut dev = fake_device()?;
        dev.peer.endpoint = None;
        let payload = build_uapi_set(&dev)?;
        assert!(!payload.contains("endpoint="));
        Ok(())
    }

    #[test]
    fn hex_encode_pads_zeros() {
        assert_eq!(hex_encode(&[0x00, 0x0a, 0xff]), "000aff");
        assert_eq!(hex_encode(&[]), "");
    }
}
