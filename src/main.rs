use std::io::Result;

use clap::Parser;
use rsproxy::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    rsproxy::run(cli).await
}
