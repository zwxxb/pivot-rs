use std::io::Result;

use clap::Parser;
use rsproxy::Cli;
use tracing::error;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    if let Err(e) = rsproxy::run(cli).await {
        error!("error: {}", e);
    }

    Ok(())
}
