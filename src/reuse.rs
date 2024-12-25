use std::{io::Result, net::SocketAddr};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    time,
};
use tracing::{error, info, warn};

use crate::tcp;

pub struct Reuse {
    local_addr: String,
    remote_addr: String,
    fallback_addr: Option<String>,
    external_ip: String,
    timeout: Option<u64>,
}

impl Reuse {
    pub fn new(
        local_addr: String,
        remote_addr: String,
        fallback_addr: Option<String>,
        external_ip: String,
        timeout: Option<u64>,
    ) -> Self {
        Self {
            local_addr,
            remote_addr,
            fallback_addr,
            external_ip,
            timeout,
        }
    }

    pub async fn start(&self) -> Result<()> {
        self.reuse_tcp().await
    }

    async fn reuse_tcp(&self) -> Result<()> {
        let local_addr: SocketAddr = self.local_addr.parse().unwrap();

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
        socket.set_reuse_address(true)?;

        #[cfg(target_family = "unix")]
        socket.set_reuse_port(true)?;

        socket.set_nonblocking(true)?;
        socket.bind(&local_addr.into())?;
        socket.listen(128)?;

        let (tx, mut rx) = mpsc::channel(1);

        let reuse_task = async move {
            let listener = TcpListener::from_std(socket.into()).expect("Failed to listen");
            info!("Bind to {} success", local_addr);

            loop {
                let (client_stream, client_addr) = listener
                    .accept()
                    .await
                    .expect("Failed to accept connection");

                info!("Accepted connection from: {}", client_addr);
                tx.send((client_stream, client_addr)).await.unwrap();
            }
        };

        match self.timeout {
            Some(timeout) => {
                tokio::spawn(time::timeout(
                    time::Duration::from_secs(timeout),
                    reuse_task,
                ));
            }
            None => {
                tokio::spawn(reuse_task);
            }
        }

        let mut alive_tasks = Vec::new();

        while let Some((client_stream, client_addr)) = rx.recv().await {
            let server_addr = if client_addr.ip().to_string() == self.external_ip {
                info!("Redirecting connection to {}", &self.remote_addr);
                &self.remote_addr
            } else {
                match &self.fallback_addr {
                    Some(fallback_addr) => {
                        warn!("Invalid external IP, fallback to {}", fallback_addr);
                        fallback_addr
                    }
                    None => {
                        warn!("Invalid external IP, abort the connection");
                        continue;
                    }
                }
            };

            let server_stream = TcpStream::connect(&server_addr)
                .await
                .expect(&format!("Failed to connect to {}", server_addr));

            info!("Connect to {} success", server_addr);

            let task = tokio::spawn(async move {
                let client_stream = tcp::NetStream::Tcp(client_stream);
                let remote_stream = tcp::NetStream::Tcp(server_stream);

                info!("Open pipe: {} <=> {}", client_addr, local_addr);
                if let Err(e) = tcp::handle_forward(client_stream, remote_stream).await {
                    error!("Failed to forward: {}", e)
                }
                info!("Close pipe: {} <=> {}", client_addr, local_addr);
            });

            alive_tasks.push(task);
        }

        if let Some(timeout) = self.timeout {
            info!(
                "Stop accepting new connections after {} elapsed, wait for alive tasks",
                timeout
            )
        };

        for task in alive_tasks {
            task.await.expect("Failed to join task");
        }

        Ok(())
    }
}
