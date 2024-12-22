use std::io::{Error, Result};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tracing::info;

use crate::{
    generate_random_string,
    tcp::{self, NetStream},
};

#[derive(Clone)]
pub struct AuthInfo {
    pub user: String,
    pub pass: String,
}

impl AuthInfo {
    pub fn new(s: String) -> Self {
        let (user, pass) = match s.contains(':') {
            true => {
                let (r1, r2) = s.split_once(':').unwrap();
                (r1.to_string(), r2.to_string())
            }
            false => (generate_random_string(12), generate_random_string(12)),
        };

        info!("user: {} pass: {}", user, pass);

        Self { user, pass }
    }
}

pub async fn handle_connection(stream: NetStream, auth_info: &Option<AuthInfo>) -> Result<()> {
    let (mut reader, mut writer) = stream.split();

    // 1. auth negotiation
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf).await?;

    if buf[0] != 0x05 {
        return Err(Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid SOCKS5 protocol version",
        ));
    }

    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    reader.read_exact(&mut methods).await?;

    match auth_info {
        Some(auth) => {
            // check username and password authentication
            if !methods.contains(&0x02) {
                writer.write_all(&[0x05, 0xff]).await?;
                return Err(Error::new(
                    std::io::ErrorKind::InvalidData,
                    "No supported authentication method",
                ));
            }

            writer.write_all(&[0x05, 0x02]).await?;

            let mut auth_buf = [0u8; 2];
            reader.read_exact(&mut auth_buf).await?;

            if auth_buf[0] != 0x01 {
                return Err(Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid authentication version",
                ));
            }

            // read username
            let ulen = auth_buf[1] as usize;
            let mut username = vec![0u8; ulen];
            reader.read_exact(&mut username).await?;

            // read password
            let plen = reader.read_u8().await? as usize;
            let mut password = vec![0u8; plen];
            reader.read_exact(&mut password).await?;

            // check username and password
            if String::from_utf8_lossy(&username) == auth.user
                && String::from_utf8_lossy(&password) == auth.pass
            {
                writer.write_all(&[0x01, 0x00]).await?;
            } else {
                writer.write_all(&[0x01, 0x01]).await?;
                return Err(Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Authentication failed",
                ));
            }
        }
        None => {
            // no auth required
            writer.write_all(&[0x05, 0x00]).await?;
        }
    }

    // 2. handle request
    let mut header = [0u8; 4];
    reader.read_exact(&mut header).await?;

    if header[0] != 0x05 {
        return Err(Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid SOCKS5 request",
        ));
    }

    if header[1] != 0x01 {
        return Err(Error::new(
            std::io::ErrorKind::Unsupported,
            "Only CONNECT command supported",
        ));
    }

    let addr = match header[3] {
        0x01 => {
            // IPv4
            let mut addr = [0u8; 4];
            reader.read_exact(&mut addr).await?;
            let mut port = [0u8; 2];
            reader.read_exact(&mut port).await?;
            format!(
                "{}.{}.{}.{}:{}",
                addr[0],
                addr[1],
                addr[2],
                addr[3],
                u16::from_be_bytes(port)
            )
        }
        0x03 => {
            // domain
            let len = reader.read_u8().await? as usize;
            let mut domain = vec![0u8; len];
            reader.read_exact(&mut domain).await?;
            let mut port = [0u8; 2];
            reader.read_exact(&mut port).await?;
            format!(
                "{}:{}",
                String::from_utf8_lossy(&domain),
                u16::from_be_bytes(port)
            )
        }
        0x04 => {
            return Err(Error::new(
                std::io::ErrorKind::Unsupported,
                "IPv6 address not supported",
            ));
        }
        _ => {
            return Err(Error::new(
                std::io::ErrorKind::Unsupported,
                "Unsupported address type",
            ))
        }
    };

    // 3. connect to the target server
    let target = NetStream::Tcp(match TcpStream::connect(&addr).await {
        Ok(stream) => stream,
        Err(e) => {
            writer
                .write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            return Err(e.into());
        }
    });

    // 4. send success response
    writer
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    // 5. forward data
    tcp::handle_forward_splitted(reader, writer, target).await
}
