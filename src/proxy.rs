//! Core network proxy logic, for passing
//! traffic between TCP sockets. Right now, only TCP is supported,
//! but UDP support would be dope.
//!
//! The methods exposed here are low-level. More user-friendly abstractions
//! can be found in the [crate::manager::TunnelManager] class..

use anyhow::Result;
use futures::FutureExt;
use std::net::SocketAddr;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

// Taken from Tokio proxy example (MIT license):
// https://github.com/tokio-rs/tokio/blob/a08ce0d3e06d650361283dc87c8fe14b146df15d/examples/proxy.rs
/// Handle proxying traffic along a given `TcpStream` to a given
/// destination socket.
pub async fn transfer(mut inbound: TcpStream, proxy_addr: SocketAddr) -> Result<()> {
    let mut outbound = TcpStream::connect(proxy_addr).await?;

    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = async {
        tokio::io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    let server_to_client = async {
        tokio::io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    tokio::try_join!(client_to_server, server_to_client)?;

    Ok(())
}

/// Create a blocking service proxy that passes TCP traffic
/// between two sockets.
pub async fn proxy_handler(listen_addr: SocketAddr, dest_addr: SocketAddr) -> Result<()> {
    tracing::debug!("Proxying traffic: {} -> {}", listen_addr, dest_addr);
    let listener = tokio::net::TcpListener::bind(&listen_addr).await?;
    while let Ok((inbound, _)) = listener.accept().await {
        let transfer = transfer(inbound, dest_addr).map(|r| {
            if let Err(e) = r {
                tracing::warn!("Proxy connection dropped, creating new handler: {}", e);
            }
        });
        tokio::spawn(transfer);
    }
    Ok(())
}
