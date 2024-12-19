use std::io::Result;

use tokio::{net::TcpStream, select};
use tracing::error;

pub async fn handle_forward(mut stream1: TcpStream, mut stream2: TcpStream) -> Result<()> {
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
        _ = handle1 => { Ok(()) },
        _ = handle2 => { Ok(()) },
    }
}
