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
    /// Port forwarding mode
    Fwd {
        /// Local listen address, format: [+][IP:]PORT
        #[arg(short, long)]
        local: Vec<String>,

        /// Remote connect address, format: [+]IP:PORT
        #[arg(short, long)]
        remote: Vec<String>,

        /// Unix domain socket path
        #[arg(short, long)]
        socket: Option<String>,

        /// Enable UDP forward mode
        #[arg(short, long)]
        udp: bool,
    },

    /// Socks proxy mode
    Socks {
        /// Local listen address, format: [+][IP:]PORT
        #[arg(short, long)]
        local: Vec<String>,

        /// Reverse server address, format: [+]IP:PORT
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
            mut remote,
            socket,
            udp,
        } => {
            info!("Starting forward mode");

            if udp {
                info!("Using UDP protocol");
            } else {
                info!("Using TCP protocol");
            }

            let mut local_ssl_opts = Vec::new();
            let mut remote_ssl_opts = Vec::new();

            for addr in &mut local {
                if addr.starts_with('+') {
                    *addr = addr.replace("+", "");
                    local_ssl_opts.push(true);
                } else {
                    local_ssl_opts.push(false);
                }

                if !addr.contains(":") {
                    *addr = "0.0.0.0:".to_string() + addr;
                }
            }

            for addr in &mut remote {
                if addr.starts_with('+') {
                    *addr = addr.replace("+", "");
                    remote_ssl_opts.push(true);
                } else {
                    remote_ssl_opts.push(false);
                }
            }

            let forward = Forward::new(local, remote, local_ssl_opts, remote_ssl_opts, socket, udp);
            forward.start().await?;
        }
        Commands::Socks { mut local, remote } => {
            info!("Starting proxy mode");

            let mut local_ssl_opts = Vec::new();
            let mut remote_ssl_opt = false;

            for addr in &mut local {
                if addr.starts_with('+') {
                    *addr = addr.replace("+", "");
                    local_ssl_opts.push(true);
                } else {
                    local_ssl_opts.push(false);
                }

                if !addr.contains(":") {
                    *addr = "0.0.0.0:".to_string() + addr;
                }
            }

            let remote = match remote {
                Some(remote) => {
                    if remote.starts_with('+') {
                        remote_ssl_opt = true;
                        Some(remote.replace("+", ""))
                    } else {
                        Some(remote)
                    }
                }
                None => None,
            };

            let proxy = Proxy::new(local, remote, local_ssl_opts, remote_ssl_opt);
            proxy.start().await?;
        }
    }

    Ok(())
}
