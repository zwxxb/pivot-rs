use std::{io::Result, sync::Arc};

use rustls::pki_types::ServerName;
use tokio::net::{TcpListener, TcpStream, UdpSocket, UnixStream};
use tracing::{error, info};

use crate::{crypto, tcp, udp};

pub struct Forward {
    local_addrs: Vec<String>,
    remote_addrs: Vec<String>,
    socket: Option<String>,
    udp: bool,
    local_opts: Vec<bool>,
    remote_opts: Vec<bool>,
}

impl Forward {
    pub fn new(
        local_addrs: Vec<String>,
        remote_addrs: Vec<String>,
        local_opts: Vec<bool>,
        remote_opts: Vec<bool>,
        socket: Option<String>,
        udp: bool,
    ) -> Self {
        Self {
            local_addrs,
            remote_addrs,
            local_opts,
            remote_opts,
            socket,
            udp,
        }
    }

    pub async fn start(&self) -> Result<()> {
        match (
            self.local_addrs.len(),
            self.remote_addrs.len(),
            &self.socket,
        ) {
            (2, 0, None) => self.local_to_local().await?,
            (1, 1, None) => self.local_to_remote().await?,
            (0, 2, None) => self.remote_to_remote().await?,
            (1, 0, Some(_)) => self.socket_to_local_tcp().await?,
            (0, 1, Some(_)) => self.socket_to_remote_tcp().await?,
            _ => error!("Invalid forward parameters"),
        }
        Ok(())
    }

    async fn local_to_local(&self) -> Result<()> {
        if self.udp {
            self.local_to_local_udp().await
        } else {
            self.local_to_local_tcp().await
        }
    }

    async fn local_to_remote(&self) -> Result<()> {
        if self.udp {
            self.local_to_remote_udp().await
        } else {
            self.local_to_remote_tcp().await
        }
    }

    async fn remote_to_remote(&self) -> Result<()> {
        if self.udp {
            self.remote_to_remote_udp().await
        } else {
            self.remote_to_remote_tcp().await
        }
    }

    async fn local_to_local_tcp(&self) -> Result<()> {
        let listener1 = TcpListener::bind(&self.local_addrs[0]).await?;
        let listener2 = TcpListener::bind(&self.local_addrs[1]).await?;

        info!("Bind to {} success", listener1.local_addr()?);
        info!("Bind to {} success", listener1.local_addr()?);

        let acceptor1 = Arc::new(match self.local_opts[0] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[0])),
            false => None,
        });

        let acceptor2 = Arc::new(match self.local_opts[1] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[1])),
            false => None,
        });

        loop {
            let (stream1, addr1) = listener1.accept().await?;
            let (stream2, addr2) = listener2.accept().await?;

            info!("Accept connection from {}", addr1);
            info!("Accept connection from {}", addr2);

            let acceptor1 = acceptor1.clone();
            let acceptor2 = acceptor2.clone();

            tokio::spawn(async move {
                let stream1 = tcp::NetStream::from_acceptor(stream1, acceptor1).await;
                let stream2 = tcp::NetStream::from_acceptor(stream2, acceptor2).await;

                if let Err(e) = tcp::handle_forward(stream1, stream2).await {
                    error!("Failed to forward: {}", e)
                }
            });
        }
    }

    async fn local_to_remote_tcp(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", listener.local_addr()?);

        let acceptor = Arc::new(match self.local_opts[0] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[0])),
            false => None,
        });

        let connector = Arc::new(match self.remote_opts[0] {
            true => Some(crypto::get_tls_connector()),
            false => None,
        });

        let (host, _) = &self.remote_addrs[0].split_once(':').unwrap();
        let domain = ServerName::try_from(host.to_string()).unwrap();

        loop {
            let (stream, addr) = listener.accept().await?;
            let remote = TcpStream::connect(&self.remote_addrs[0]).await?;

            let acceptor = acceptor.clone();
            let connector = connector.clone();
            let domain = domain.clone();

            info!("Accept connection from {}", addr);
            info!("Connect to {} success", remote.peer_addr()?);

            tokio::spawn(async move {
                let stream = tcp::NetStream::from_acceptor(stream, acceptor).await;
                let remote = tcp::NetStream::from_connector(remote, domain, connector).await;

                if let Err(e) = tcp::handle_forward(stream, remote).await {
                    error!("failed to forward: {}", e)
                }
            });
        }
    }

    async fn remote_to_remote_tcp(&self) -> Result<()> {
        let connector1 = Arc::new(match self.remote_opts[0] {
            true => Some(crypto::get_tls_connector()),
            false => None,
        });

        let connector2 = Arc::new(match self.remote_opts[1] {
            true => Some(crypto::get_tls_connector()),
            false => None,
        });

        let (host1, _) = &self.remote_addrs[0].split_once(':').unwrap();
        let domain1 = ServerName::try_from(host1.to_string()).unwrap();

        let (host2, _) = &self.remote_addrs[1].split_once(':').unwrap();
        let domain2 = ServerName::try_from(host2.to_string()).unwrap();

        loop {
            let stream1 = TcpStream::connect(&self.remote_addrs[0]).await?;
            let stream2 = TcpStream::connect(&self.remote_addrs[1]).await?;

            info!("Connect to {} success", stream1.peer_addr()?);
            info!("Connect to {} success", stream2.peer_addr()?);

            let connector1 = connector1.clone();
            let connector2 = connector2.clone();
            let domain1 = domain1.clone();
            let domain2 = domain2.clone();

            let stream1 = tcp::NetStream::from_connector(stream1, domain1, connector1).await;
            let stream2 = tcp::NetStream::from_connector(stream2, domain2, connector2).await;

            tcp::handle_forward(stream1, stream2).await?;
        }
    }

    async fn socket_to_local_tcp(&self) -> Result<()> {
        let socket_path = self.socket.as_ref().unwrap();

        let local_listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", local_listener.local_addr()?);

        let acceptor = Arc::new(match self.local_opts[0] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[0])),
            false => None,
        });

        loop {
            let (local_stream, addr) = local_listener.accept().await?;
            let unix_stream = UnixStream::connect(socket_path).await?;

            info!("Accept connection from {}", addr);
            info!("Connect to {} success", socket_path);

            let acceptor = acceptor.clone();

            tokio::spawn(async move {
                let local_stream = tcp::NetStream::from_acceptor(local_stream, acceptor).await;
                let unix_stream = tcp::NetStream::Unix(unix_stream);

                if let Err(e) = tcp::handle_forward(unix_stream, local_stream).await {
                    error!("Failed to forward: {}", e)
                }
            });
        }
    }

    async fn socket_to_remote_tcp(&self) -> Result<()> {
        let socket_path = self.socket.as_ref().unwrap();

        let connector = Arc::new(match self.remote_opts[0] {
            true => Some(crypto::get_tls_connector()),
            false => None,
        });

        let (host, _) = &self.remote_addrs[0].split_once(':').unwrap();
        let domain = ServerName::try_from(host.to_string()).unwrap();

        loop {
            let unix_stream = UnixStream::connect(socket_path).await?;
            let remote_stream = TcpStream::connect(&self.remote_addrs[0]).await?;

            info!("Connect to {} success", socket_path);
            info!("Connect to {} success", remote_stream.peer_addr()?);

            let connector = connector.clone();
            let domain = domain.clone();

            let unix_stream = tcp::NetStream::Unix(unix_stream);
            let remote_stream =
                tcp::NetStream::from_connector(remote_stream, domain, connector).await;

            tcp::handle_forward(unix_stream, remote_stream).await?;
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
