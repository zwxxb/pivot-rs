use std::io::Result;
use std::sync::Arc;

use rustls::pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::{
    io,
    net::{TcpStream, UnixStream},
    select,
};
use tokio_rustls::{client, server, TlsAcceptor, TlsConnector};
use tracing::error;

pub enum NetStream {
    Tcp(TcpStream),
    Unix(UnixStream),
    ServerTls(server::TlsStream<TcpStream>),
    ClientTls(client::TlsStream<TcpStream>),
}

impl NetStream {
    pub async fn from_acceptor(stream: TcpStream, acceptor: Arc<Option<TlsAcceptor>>) -> Self {
        match acceptor.as_ref() {
            Some(acceptor) => Self::ServerTls(acceptor.accept(stream).await.unwrap()),
            None => Self::Tcp(stream),
        }
    }

    pub async fn from_connector(stream: TcpStream, connector: Arc<Option<TlsConnector>>) -> Self {
        match connector.as_ref() {
            Some(connector) => Self::ClientTls(
                connector
                    .connect(ServerName::try_from("localhost").unwrap(), stream)
                    .await
                    .unwrap(),
            ),
            None => Self::Tcp(stream),
        }
    }

    pub fn split(
        self,
    ) -> (
        Box<dyn AsyncRead + Unpin + Send>,
        Box<dyn AsyncWrite + Unpin + Send>,
    ) {
        match self {
            NetStream::Tcp(stream) => {
                let (r, w) = io::split(stream);
                (Box::new(r), Box::new(w))
            }
            NetStream::Unix(stream) => {
                let (r, w) = io::split(stream);
                (Box::new(r), Box::new(w))
            }
            NetStream::ServerTls(stream) => {
                let (r, w) = io::split(stream);
                (Box::new(r), Box::new(w))
            }
            NetStream::ClientTls(stream) => {
                let (r, w) = io::split(stream);
                (Box::new(r), Box::new(w))
            }
        }
    }
}

pub async fn handle_forward(stream1: NetStream, stream2: NetStream) -> Result<()> {
    let (r1, w1) = stream1.split();

    handle_forward_splitted(r1, w1, stream2).await
}

pub async fn handle_forward_splitted(
    mut r1: Box<dyn AsyncRead + Send + Unpin>,
    mut w1: Box<dyn AsyncWrite + Send + Unpin>,
    stream2: NetStream,
) -> Result<()> {
    let (mut r2, mut w2) = stream2.split();

    let handle1 = async {
        if let Err(e) = tokio::io::copy(&mut r1, &mut w2).await {
            error!("Failed to copy: {}", e);
        }
    };

    let handle2 = async {
        if let Err(e) = tokio::io::copy(&mut r2, &mut w1).await {
            error!("Failed to copy: {}", e);
        }
    };

    select! {
        _ = handle1 => {},
        _ = handle2 => {},
    }

    Ok(())
}
