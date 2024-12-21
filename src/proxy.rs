use std::{io::Result, sync::Arc};

use rustls::pki_types::ServerName;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use crate::{
    crypto,
    socks::handle_connection,
    tcp::{self},
};

pub struct Proxy {
    local_addrs: Vec<String>,
    remote_addr: Option<String>,
    local_opts: Vec<bool>,
    remote_opt: bool,
}

impl Proxy {
    pub fn new(
        local_addrs: Vec<String>,
        remote_addr: Option<String>,
        local_opts: Vec<bool>,
        remote_opt: bool,
    ) -> Self {
        Self {
            local_addrs,
            remote_addr,
            local_opts,
            remote_opt,
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
        info!("Start socks server on {}", listener.local_addr()?);

        let acceptor = Arc::new(match self.local_opts[0] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[0])),
            false => None,
        });

        loop {
            let (stream, addr) = listener.accept().await?;
            info!("Accept connection from {}", addr);

            let acceptor = acceptor.clone();

            tokio::spawn(async move {
                let stream = match acceptor.as_ref() {
                    Some(acceptor) => {
                        tcp::NetStream::ServerTls(acceptor.accept(stream).await.unwrap())
                    }
                    None => tcp::NetStream::Tcp(stream),
                };

                if let Err(e) = handle_connection(stream).await {
                    error!("Failed to handle connection: {}", e);
                }
            });
        }
    }

    pub async fn socks_reverse_client(&self) -> Result<()> {
        let remote_addr = self.remote_addr.clone().unwrap();

        let connector = Arc::new(match self.remote_opt {
            true => Some(crypto::get_tls_connector()),
            false => None,
        });

        let (host, _) = remote_addr.split_once(':').unwrap();
        let domain = ServerName::try_from(host.to_string()).unwrap();

        loop {
            match TcpStream::connect(&remote_addr).await {
                Ok(stream) => {
                    info!("Connect to remote {} success", stream.peer_addr()?);

                    let connector = connector.clone();
                    let domain = domain.clone();

                    tokio::spawn(async move {
                        let stream = match connector.as_ref() {
                            Some(connector) => tcp::NetStream::ClientTls(
                                connector.connect(domain, stream).await.unwrap(),
                            ),
                            None => tcp::NetStream::Tcp(stream),
                        };

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

        info!("Bind to {} success", control_listener.local_addr()?);
        info!("Bind to {} success", proxy_listener.local_addr()?);

        let control_acceptor = Arc::new(match self.local_opts[0] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[0])),
            false => None,
        });

        let proxy_acceptor = Arc::new(match self.local_opts[1] {
            true => Some(crypto::get_tls_acceptor(&self.local_addrs[1])),
            false => None,
        });

        loop {
            let (proxy_stream, proxy_addr) = proxy_listener.accept().await?;
            let (control_stream, control_addr) = control_listener.accept().await?;

            info!("Accept connection from {}", proxy_addr);
            info!("Accept connection from {}", control_addr);

            let proxy_acceptor = proxy_acceptor.clone();
            let control_acceptor = control_acceptor.clone();

            tokio::spawn(async move {
                let proxy_stream = match proxy_acceptor.as_ref() {
                    Some(acceptor) => {
                        tcp::NetStream::ServerTls(acceptor.accept(proxy_stream).await.unwrap())
                    }
                    None => tcp::NetStream::Tcp(proxy_stream),
                };

                let control_stream = match control_acceptor.as_ref() {
                    Some(acceptor) => {
                        tcp::NetStream::ServerTls(acceptor.accept(control_stream).await.unwrap())
                    }
                    None => tcp::NetStream::Tcp(control_stream),
                };

                if let Err(e) = tcp::handle_forward(proxy_stream, control_stream).await {
                    error!("Failed to handle forward: {}", e);
                }
            });
        }
    }
}
