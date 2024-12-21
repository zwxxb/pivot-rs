use clap::Parser;
use rsproxy::Cli;
use std::io::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    rsproxy::run(cli).await
}
