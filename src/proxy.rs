use std::io::Result;

use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use crate::{
    socks::handle_connection,
    tcp::{self},
};

pub struct Proxy {
    local_addrs: Vec<String>,
    remote_addr: Option<String>,
}

impl Proxy {
    pub fn new(local_addrs: Vec<String>, remote_addr: Option<String>) -> Self {
        Self {
            local_addrs,
            remote_addr,
        }
    }

    pub async fn start(&self) -> Result<()> {
        match (self.local_addrs.len(), &self.remote_addr) {
            (1, None) => self.socks_server().await?,
            (2, None) => self.socks_reverse_server().await?,
            (0, Some(_)) => self.socks_reverse_client().await?,
            _ => error!("Invalid proxy parameters"),
        }

        Ok(())
    }

    pub async fn socks_server(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Start socks server on {}", self.local_addrs[0]);

        loop {
            let (stream, addr) = listener.accept().await?;
            info!("Accept connection from {}", addr);

            tokio::spawn(async {
                if let Err(e) = handle_connection(stream).await {
                    error!("Failed to handle connection: {}", e);
                }
            });
        }
    }

    pub async fn socks_reverse_client(&self) -> Result<()> {
        let remote_addr = self.remote_addr.clone().unwrap();

        loop {
            match TcpStream::connect(&remote_addr).await {
                Ok(stream) => {
                    info!("Connect to remote {} success", remote_addr);

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream).await {
                            error!("Failed to handle connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to establish connection: {}", e);
                    continue;
                }
            }
        }
    }

    pub async fn socks_reverse_server(&self) -> Result<()> {
        let control_listener = TcpListener::bind(&self.local_addrs[0]).await?;
        let proxy_listener = TcpListener::bind(&self.local_addrs[1]).await?;

        loop {
            let (proxy_stream, proxy_addr) = proxy_listener.accept().await?;
            let (control_stream, control_addr) = control_listener.accept().await?;

            info!("Accept connection from {}", proxy_addr);
            info!("Accept connection from {}", control_addr);

            tokio::spawn(async move {
                if let Err(e) = tcp::handle_forward(proxy_stream, control_stream).await {
                    error!("Failed to handle forward: {}", e);
                }
            });
        }
    }
}
