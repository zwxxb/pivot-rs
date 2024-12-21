use std::io::{Error, Result};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    select,
};
use tracing::error;

use crate::tcp::NetStream;

pub async fn handle_connection(stream: NetStream) -> Result<()> {
    let (mut reader, mut writer) = stream.split();

    // 1. 认证协商
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

    // 不需要认证
    writer.write_all(&[0x05, 0x00]).await?;

    // 2. 请求处理
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
            // 域名
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

    // 3. 连接目标服务器
    let target = NetStream::Tcp(match TcpStream::connect(&addr).await {
        Ok(stream) => stream,
        Err(e) => {
            writer
                .write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            return Err(e.into());
        }
    });

    // 4. 发送连接成功响应
    writer
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    // 5. 转发数据
    handle_forward(reader, writer, target).await
}

pub async fn handle_forward(
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
