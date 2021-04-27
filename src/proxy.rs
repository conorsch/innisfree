use futures::FutureExt;
use std::error::Error;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

// Taken from Tokio proxy example (MIT license):
// https://github.com/tokio-rs/tokio/blob/a08ce0d3e06d650361283dc87c8fe14b146df15d/examples/proxy.rs
pub async fn transfer(mut inbound: TcpStream, proxy_addr: String) -> Result<(), Box<dyn Error>> {
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

pub async fn proxy_handler(listen_addr: String, dest_addr: String) -> Result<(), Box<dyn Error>> {
    debug!("Proxying traffic: {} -> {}", listen_addr, dest_addr);
    let listener = tokio::net::TcpListener::bind(listen_addr.clone()).await?;
    while let Ok((inbound, _)) = listener.accept().await {
        let transfer = transfer(inbound, dest_addr.clone()).map(|r| {
            if let Err(e) = r {
                error!("Proxy logic failed: {}", e);
                // It's bonkers to exit, but still debugging. So would like
                // to catch this error in the wild. Likely an ugly exit, no cleanup.
                error!("EXITING");
                std::process::exit(2);
            }
        });
        tokio::spawn(transfer);
    }
    Ok(())
}
