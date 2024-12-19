use std::io::Result;

use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use crate::tcp::handle_forward;

pub struct Forward {
    local_addrs: Vec<String>,
    remote_addrs: Vec<String>,
}

impl Forward {
    pub fn new(local_addrs: Vec<String>, remote_addrs: Vec<String>) -> Self {
        Self {
            local_addrs,
            remote_addrs,
        }
    }

    pub async fn start(&self) -> Result<()> {
        match (self.local_addrs.len(), self.remote_addrs.len()) {
            (2, 0) => self.local_to_local().await?,
            (1, 1) => self.local_to_remote().await?,
            (0, 2) => self.remote_to_remote().await?,
            _ => error!("Invalid forward parameters"),
        }
        Ok(())
    }

    pub async fn local_to_local(&self) -> Result<()> {
        let listener1 = TcpListener::bind(&self.local_addrs[0]).await?;
        let listener2 = TcpListener::bind(&self.local_addrs[1]).await?;

        info!("Bind to {} success", self.local_addrs[0]);
        info!("Bind to {} success", self.local_addrs[1]);

        loop {
            let (stream1, addr1) = listener1.accept().await?;
            let (stream2, addr2) = listener2.accept().await?;

            info!("Accept connection from {}", addr1);
            info!("Accept connection from {}", addr2);

            tokio::spawn(async {
                if let Err(e) = handle_forward(stream1, stream2).await {
                    error!("Failed to forward: {}", e)
                }
            });
        }
    }

    pub async fn local_to_remote(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", self.local_addrs[0]);

        loop {
            let (stream, addr) = listener.accept().await?;
            let remote = TcpStream::connect(&self.remote_addrs[0]).await?;

            info!("Accept connection from {}", addr);
            info!("Connect to {} success", self.remote_addrs[0]);

            tokio::spawn(async {
                if let Err(e) = handle_forward(stream, remote).await {
                    error!("failed to forward: {}", e)
                }
            });
        }
    }

    pub async fn remote_to_remote(&self) -> Result<()> {
        loop {
            let stream1 = TcpStream::connect(&self.remote_addrs[0]).await?;
            let stream2 = TcpStream::connect(&self.remote_addrs[1]).await?;

            info!("Connect to {} success", self.remote_addrs[0]);
            info!("Connect to {} success", self.remote_addrs[1]);

            handle_forward(stream1, stream2).await?;
        }
    }
}
