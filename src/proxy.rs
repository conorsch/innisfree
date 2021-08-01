use crate::config::InnisfreeError;
use futures::FutureExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

// Taken from Tokio proxy example (MIT license):
// https://github.com/tokio-rs/tokio/blob/a08ce0d3e06d650361283dc87c8fe14b146df15d/examples/proxy.rs
pub async fn transfer(mut inbound: TcpStream, proxy_addr: String) -> Result<(), InnisfreeError> {
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

pub async fn proxy_handler(listen_addr: String, dest_addr: String) -> Result<(), InnisfreeError> {
    debug!("Proxying traffic: {} -> {}", listen_addr, dest_addr);
    let listener = tokio::net::TcpListener::bind(listen_addr.clone()).await?;
    while let Ok((inbound, _)) = listener.accept().await {
        let transfer = transfer(inbound, dest_addr.clone()).map(|r| {
            if let Err(e) = r {
                warn!("Proxy connection dropped, creating new handler: {}", e);
            }
        });
        tokio::spawn(transfer);
    }
    Ok(())
}
