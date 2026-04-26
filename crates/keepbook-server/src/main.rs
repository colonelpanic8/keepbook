use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use keepbook_server::{default_listen_addr, default_server_config_path};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "keepbook-server")]
#[command(about = "Serve the keepbook HTTP API for Rust UI clients")]
struct Cli {
    /// Path to keepbook.toml
    #[arg(short, long, default_value_os_t = default_server_config_path())]
    config: PathBuf,

    /// Address to listen on
    #[arg(long, default_value_t = default_listen_addr())]
    addr: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();

    let cli = Cli::parse();
    keepbook_server::serve(cli.config, cli.addr).await
}
