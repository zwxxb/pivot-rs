use std::io::Result;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::{
    io,
    net::{TcpStream, UnixStream},
    select,
};
use tokio_rustls::{client, server};
use tracing::error;

pub enum NetStream {
    Tcp(TcpStream),
    Unix(UnixStream),
    ServerTls(server::TlsStream<TcpStream>),
    ClientTls(client::TlsStream<TcpStream>),
}

impl NetStream {
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
    let (mut r1, mut w1) = stream1.split();
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

// pub async fn handle_unix_socket_forward(
//     mut unix_stream: UnixStream,
//     mut tcp_stream: TcpStream,
// ) -> Result<()> {
//     let (mut tcp_reader, mut tcp_writer) = tcp_stream.split();
//     let (mut unix_reader, mut unix_writer) = unix_stream.split();

//     let handle1 = async {
//         if let Err(e) = tokio::io::copy(&mut tcp_reader, &mut unix_writer).await {
//             error!("Failed to copy: {}", e);
//         }
//     };

//     let handle2 = async {
//         if let Err(e) = tokio::io::copy(&mut unix_reader, &mut tcp_writer).await {
//             error!("Failed to copy: {}", e);
//         }
//     };

//     select! {
//         _ = handle1 => {},
//         _ = handle2 => {},
//     }

//     Ok(())
// }
