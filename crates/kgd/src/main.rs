mod config;
mod discord;
mod ping;
mod status;
mod version;
mod wol;

use std::{path::PathBuf, time::Duration};

use anyhow::{Context as _, Result};
use clap::Parser;
use tokio::sync::mpsc;
use tracing::info;

use crate::{
    config::{open_config, write_default_config},
    version::short_version,
};

#[derive(Parser)]
#[command(version = short_version())]
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

    tracing::info!(version = short_version(), "kgd version");

    let config = open_config(&args.config).context("Failed to load configuration")?;
    info!(servers = config.servers.len(), "Configuration loaded");

    let (status_tx, status_rx) = mpsc::channel(1);

    let servers = config.servers.clone();
    let interval = config.status.interval;
    tokio::spawn(run_status_monitor(servers, interval, status_tx));

    discord::run(config, status_rx).await
}

/// サーバーステータスを定期的にチェックし、結果をチャンネルに送信するループを実行する。
///
/// # Arguments
/// * `servers` - 監視対象のサーバー設定リスト
/// * `interval` - チェック間隔
/// * `tx` - ステータス結果を送信するチャンネル
async fn run_status_monitor(
    servers: Vec<config::ServerConfig>,
    interval: Duration,
    tx: mpsc::Sender<Vec<status::ServerStatus>>,
) {
    let ping_timeout = Duration::from_secs(5);

    info!(interval = ?interval, "Starting status monitor");

    loop {
        let statuses = status::check_servers(&servers, ping_timeout).await;
        if tx.send(statuses).await.is_err() {
            break;
        }
        tokio::time::sleep(interval).await;
    }
}
