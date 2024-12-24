use std::{io::Result, sync::Arc};

use tokio::{
    join,
    net::{TcpListener, TcpStream, UdpSocket},
    sync,
};
use tracing::{error, info};

#[cfg(target_family = "unix")]
use tokio::net::UnixStream;

use crate::{crypto, tcp, udp};

pub struct Forward {
    local_addrs: Vec<String>,
    remote_addrs: Vec<String>,
    local_opts: Vec<bool>,
    remote_opts: Vec<bool>,
    #[cfg(target_family = "unix")]
    socket: Option<String>,
    udp: bool,
}

impl Forward {
    pub fn new(
        local_addrs: Vec<String>,
        remote_addrs: Vec<String>,
        local_opts: Vec<bool>,
        remote_opts: Vec<bool>,
        #[cfg(target_family = "unix")] socket: Option<String>,
        udp: bool,
    ) -> Self {
        Self {
            local_addrs,
            remote_addrs,
            local_opts,
            remote_opts,
            #[cfg(target_family = "unix")]
            socket,
            udp,
        }
    }

    pub async fn start(&self) -> Result<()> {
        #[cfg(target_family = "unix")]
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

        #[cfg(target_family = "windows")]
        match (self.local_addrs.len(), self.remote_addrs.len()) {
            (2, 0) => self.local_to_local().await?,
            (1, 1) => self.local_to_remote().await?,
            (0, 2) => self.remote_to_remote().await?,
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

        let acceptor1 =
            Arc::new(self.local_opts[0].then_some(crypto::get_tls_acceptor(&self.local_addrs[0])));
        let acceptor2 =
            Arc::new(self.local_opts[1].then_some(crypto::get_tls_acceptor(&self.local_addrs[1])));

        loop {
            let (r1, r2) = join!(listener1.accept(), listener2.accept());

            let (stream1, addr1) = r1?;
            let (stream2, addr2) = r2?;

            info!("Accept connection from {}", addr1);
            info!("Accept connection from {}", addr2);

            let acceptor1 = acceptor1.clone();
            let acceptor2 = acceptor2.clone();

            tokio::spawn(async move {
                let stream1 = tcp::NetStream::from_acceptor(stream1, acceptor1).await;
                let stream2 = tcp::NetStream::from_acceptor(stream2, acceptor2).await;

                info!("Open pipe: {} <=> {}", addr1, addr2);
                if let Err(e) = tcp::handle_forward(stream1, stream2).await {
                    error!("Failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", addr1, addr2);
            });
        }
    }

    async fn local_to_remote_tcp(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", listener.local_addr()?);

        let acceptor =
            Arc::new(self.local_opts[0].then_some(crypto::get_tls_acceptor(&self.local_addrs[0])));

        let connector = Arc::new(self.remote_opts[0].then_some(crypto::get_tls_connector()));

        loop {
            let (client_stream, client_addr) = listener.accept().await?;
            let remote_stream = TcpStream::connect(&self.remote_addrs[0]).await?;

            let remote_addr = remote_stream.peer_addr()?;

            info!("Accept connection from {}", client_addr);
            info!("Connect to {} success", remote_addr);

            let acceptor = acceptor.clone();
            let connector = connector.clone();

            tokio::spawn(async move {
                let client_stream = tcp::NetStream::from_acceptor(client_stream, acceptor).await;
                let remote_stream = tcp::NetStream::from_connector(remote_stream, connector).await;

                info!("Open pipe: {} <=> {}", client_addr, remote_addr);
                if let Err(e) = tcp::handle_forward(client_stream, remote_stream).await {
                    error!("failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", client_addr, remote_addr);
            });
        }
    }

    async fn remote_to_remote_tcp(&self) -> Result<()> {
        let connector1 = Arc::new(self.remote_opts[0].then_some(crypto::get_tls_connector()));
        let connector2 = Arc::new(self.remote_opts[1].then_some(crypto::get_tls_connector()));

        // limit the number of concurrent connections
        let semaphore = Arc::new(sync::Semaphore::new(32));

        loop {
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let (r1, r2) = join!(
                TcpStream::connect(&self.remote_addrs[0]),
                TcpStream::connect(&self.remote_addrs[1])
            );

            let (stream1, stream2) = (r1?, r2?);

            let addr1 = stream1.peer_addr()?;
            let addr2 = stream2.peer_addr()?;

            info!("Connect to {} success", addr1);
            info!("Connect to {} success", addr2);

            let connector1 = connector1.clone();
            let connector2 = connector2.clone();

            tokio::spawn(async move {
                let stream1 = tcp::NetStream::from_connector(stream1, connector1).await;
                let stream2 = tcp::NetStream::from_connector(stream2, connector2).await;

                info!("Open pipe: {} <=> {}", addr1, addr2);
                if let Err(e) = tcp::handle_forward(stream1, stream2).await {
                    error!("Failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", addr1, addr2);

                // drop the permit to release the semaphore
                drop(permit);
            });
        }
    }

    #[cfg(target_family = "unix")]
    async fn socket_to_local_tcp(&self) -> Result<()> {
        let local_listener = TcpListener::bind(&self.local_addrs[0]).await?;
        info!("Bind to {} success", local_listener.local_addr()?);

        let acceptor =
            Arc::new(self.local_opts[0].then_some(crypto::get_tls_acceptor(&self.local_addrs[0])));

        loop {
            let unix_addr = self.socket.clone().unwrap();

            let (client_stream, client_addr) = local_listener.accept().await?;
            let unix_stream = UnixStream::connect(&unix_addr).await?;

            info!("Accept connection from {}", client_addr);
            info!("Connect to {} success", unix_addr);

            let acceptor = acceptor.clone();

            tokio::spawn(async move {
                let client_stream = tcp::NetStream::from_acceptor(client_stream, acceptor).await;
                let unix_stream = tcp::NetStream::Unix(unix_stream);

                info!("Open pipe: {} <=> {}", unix_addr, client_addr);
                if let Err(e) = tcp::handle_forward(client_stream, unix_stream).await {
                    error!("Failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", unix_addr, client_addr);
            });
        }
    }

    #[cfg(target_family = "unix")]
    async fn socket_to_remote_tcp(&self) -> Result<()> {
        let connector = Arc::new(self.remote_opts[0].then_some(crypto::get_tls_connector()));

        // limit the number of concurrent connections
        let semaphore = Arc::new(sync::Semaphore::new(32));

        loop {
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let unix_addr = self.socket.clone().unwrap();
            let remote_addr = self.remote_addrs[0].clone();

            let (r1, r2) = join!(
                UnixStream::connect(&unix_addr),
                TcpStream::connect(&remote_addr)
            );

            let (unix_stream, remote_stream) = (r1?, r2?);

            info!("Connect to {} success", unix_addr);
            info!("Connect to {} success", remote_addr);

            let connector = connector.clone();

            tokio::spawn(async move {
                let unix_stream = tcp::NetStream::Unix(unix_stream);
                let remote_stream = tcp::NetStream::from_connector(remote_stream, connector).await;

                info!("Open pipe: {} <=> {}", unix_addr, remote_addr);
                if let Err(e) = tcp::handle_forward(unix_stream, remote_stream).await {
                    error!("Failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", unix_addr, remote_addr);

                // drop the permit to release the semaphore
                drop(permit);
            });
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
