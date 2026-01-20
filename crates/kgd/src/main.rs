mod config;
mod discord;
mod wol;

use anyhow::{Context, Result};
use clap::Parser;
use config::{open_config, write_default_config};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "config.toml")]
    config: PathBuf,

    #[arg(long)]
    init: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    if args.init {
        write_default_config(&args.config)?;
        info!(path = ?args.config, "Created default configuration");
        return Ok(());
    }

    let config = open_config(&args.config).context("Failed to load configuration")?;
    info!(servers = config.servers.len(), "Configuration loaded");

    discord::run(config).await
}
