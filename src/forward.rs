use std::io::Result;

use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tracing::{error, info};

use crate::{tcp, udp};

pub struct Forward {
    local_addrs: Vec<String>,
    remote_addrs: Vec<String>,
    udp: bool,
}

impl Forward {
    pub fn new(local_addrs: Vec<String>, remote_addrs: Vec<String>, udp: bool) -> Self {
        Self {
            local_addrs,
            remote_addrs,
            udp,
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
        if self.udp {
            self.local_to_local_udp().await
        } else {
            self.local_to_local_tcp().await
        }
    }

    pub async fn local_to_remote(&self) -> Result<()> {
        if self.udp {
            self.local_to_remote_udp().await
        } else {
            self.local_to_remote_tcp().await
        }
    }

    pub async fn remote_to_remote(&self) -> Result<()> {
        if self.udp {
            self.remote_to_remote_udp().await
        } else {
            self.remote_to_remote_tcp().await
        }
    }

    async fn local_to_local_tcp(&self) -> Result<()> {
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
                if let Err(e) = tcp::handle_forward(stream1, stream2).await {
                    error!("Failed to forward: {}", e)
                }
            });
        }
    }

    async fn local_to_remote_tcp(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", self.local_addrs[0]);

        loop {
            let (stream, addr) = listener.accept().await?;
            let remote = TcpStream::connect(&self.remote_addrs[0]).await?;

            info!("Accept connection from {}", addr);
            info!("Connect to {} success", self.remote_addrs[0]);

            tokio::spawn(async {
                if let Err(e) = tcp::handle_forward(stream, remote).await {
                    error!("failed to forward: {}", e)
                }
            });
        }
    }

    async fn remote_to_remote_tcp(&self) -> Result<()> {
        loop {
            let stream1 = TcpStream::connect(&self.remote_addrs[0]).await?;
            let stream2 = TcpStream::connect(&self.remote_addrs[1]).await?;

            info!("Connect to {} success", self.remote_addrs[0]);
            info!("Connect to {} success", self.remote_addrs[1]);

            tcp::handle_forward(stream1, stream2).await?;
        }
    }

    async fn local_to_local_udp(&self) -> Result<()> {
        let socket1 = UdpSocket::bind(&self.local_addrs[0]).await?;
        let socket2 = UdpSocket::bind(&self.local_addrs[1]).await?;

        info!("Bind to {} success", self.local_addrs[0]);
        info!("Bind to {} success", self.local_addrs[1]);

        // socket1 will receive the handshake packet to keep client address
        udp::handle_local_forward(socket1, socket2).await
    }

    async fn local_to_remote_udp(&self) -> Result<()> {
        let local_socket = UdpSocket::bind(&self.local_addrs[0]).await?;
        let remote_socket = UdpSocket::bind("0.0.0.0:0").await?;

        remote_socket.connect(&self.remote_addrs[0]).await?;
        info!("Connect to {} success", self.remote_addrs[0]);

        udp::handle_local_to_remote_forward(local_socket, remote_socket).await
    }

    async fn remote_to_remote_udp(&self) -> Result<()> {
        let socket1 = UdpSocket::bind("0.0.0.0:0").await?;
        let socket2 = UdpSocket::bind("0.0.0.0:0").await?;

        socket1.connect(&self.remote_addrs[0]).await?;
        socket2.connect(&self.remote_addrs[1]).await?;

        info!("Connect to {} success", self.remote_addrs[0]);
        info!("Connect to {} success", self.remote_addrs[1]);

        // socket2 will send the handshake packet to keep client address
        udp::handle_remote_forward(socket1, socket2).await
    }
}
