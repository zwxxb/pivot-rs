use std::{io::Result, sync::Arc};

use tokio::{
    join,
    net::{TcpListener, TcpStream},
};
use tracing::{error, info};

use crate::{
    crypto,
    socks::{handle_connection, AuthInfo},
    tcp::{self},
};

pub struct Proxy {
    local_addrs: Vec<String>,
    remote_addr: Option<String>,
    local_opts: Vec<bool>,
    remote_opt: bool,
    auth_info: Option<AuthInfo>,
}

impl Proxy {
    pub fn new(
        local_addrs: Vec<String>,
        remote_addr: Option<String>,
        local_opts: Vec<bool>,
        remote_opt: bool,
        auth_info: Option<AuthInfo>,
    ) -> Self {
        Self {
            local_addrs,
            remote_addr,
            local_opts,
            remote_opt,
            auth_info,
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

        let auth_info = Arc::new(self.auth_info.clone());

        loop {
            let (stream, addr) = listener.accept().await?;
            info!("Accept connection from {}", addr);

            let acceptor = acceptor.clone();
            let auth_info = auth_info.clone();

            tokio::spawn(async move {
                let stream = tcp::NetStream::from_acceptor(stream, acceptor).await;

                if let Err(e) = handle_connection(stream, auth_info.as_ref()).await {
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

        let auth_info = Arc::new(self.auth_info.clone());

        // limit the number of concurrent connections
        let semaphore = Arc::new(tokio::sync::Semaphore::new(32));

        loop {
            let permit = semaphore.clone().acquire_owned().await;

            let stream = TcpStream::connect(&remote_addr).await?;
            info!("Connect to remote {} success", stream.peer_addr()?);

            let connector = connector.clone();
            let auth_info = auth_info.clone();

            tokio::spawn(async move {
                let stream = tcp::NetStream::from_connector(stream, connector).await;

                if let Err(e) = handle_connection(stream, auth_info.as_ref()).await {
                    error!("Failed to handle connection: {}", e);
                }

                // drop the permit to release the semaphore
                drop(permit);
            });
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
            let (r1, r2) = join!(proxy_listener.accept(), control_listener.accept());

            let (proxy_stream, proxy_addr) = r1?;
            let (control_stream, control_addr) = r2?;

            info!("Accept connection from {}", proxy_addr);
            info!("Accept connection from {}", control_addr);

            let proxy_acceptor = proxy_acceptor.clone();
            let control_acceptor = control_acceptor.clone();

            tokio::spawn(async move {
                let proxy_stream =
                    tcp::NetStream::from_acceptor(proxy_stream, proxy_acceptor).await;
                let control_stream =
                    tcp::NetStream::from_acceptor(control_stream, control_acceptor).await;

                info!("Open pipe: {} <=> {}", proxy_addr, control_addr);
                if let Err(e) = tcp::handle_forward(proxy_stream, control_stream).await {
                    error!("Failed to handle forward: {}", e);
                }
                info!("Close pipe: {} <=> {}", proxy_addr, control_addr);
            });
        }
    }
}
