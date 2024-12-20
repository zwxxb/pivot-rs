use std::io::Result;

use clap::{Parser, Subcommand};
use rsproxy::{forward::Forward, proxy::Proxy};
use tracing::info;

#[derive(Parser)]
#[command(author, version, about = "Rsproxy: Port-Forwarding and Proxy Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Forwarding mode
    Fwd {
        /// Local listen address, format: [IP:]PORT
        #[arg(short, long)]
        local: Vec<String>,

        /// Remote connect address, format: IP:PORT
        #[arg(short, long)]
        remote: Vec<String>,

        /// Enable UDP forward mode
        #[arg(short, long)]
        udp: bool,
    },

    /// Socks mode
    Socks {
        /// Local listen address, format: [IP:]PORT
        #[arg(short, long)]
        local: Vec<String>,

        /// Reverse server address, format: IP:PORT
        #[arg(short, long)]
        remote: Option<String>,
    },
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Fwd {
            mut local,
            remote,
            udp,
        } => {
            info!("Starting forward mode");

            if udp {
                info!("Using UDP protocol");
            } else {
                info!("Using TCP protocol");
            }

            for addr in &mut local {
                if !addr.contains(":") {
                    *addr = "0.0.0.0:".to_string() + addr;
                }
            }

            let forward = Forward::new(local, remote, udp);
            forward.start().await?;
        }
        Commands::Socks { mut local, remote } => {
            info!("Starting proxy mode");

            for addr in &mut local {
                if !addr.contains(":") {
                    *addr = "0.0.0.0:".to_string() + addr;
                }
            }

            let proxy = Proxy::new(local, remote);
            proxy.start().await?;
        }
    }

    Ok(())
}
