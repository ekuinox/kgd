mod bootstrap;
mod config;
mod version;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use clap::Parser;
use kgd_application::CheckServerStatus;
use kgd_domain::ServerStatus;
use kgd_infrastructure::SurgeProber;
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

    let ping_timeout = Duration::from_secs(5);
    let check_status =
        CheckServerStatus::new(Arc::new(SurgeProber), config.server_targets(), ping_timeout);
    let interval = config.status.interval;
    tokio::spawn(run_status_monitor(check_status, interval, status_tx));

    bootstrap::run(config, status_rx).await
}

/// サーバーステータスを定期的にチェックし、結果をチャンネルに送信するループを実行する。
///
/// # Arguments
/// * `check_status` - 死活確認ユースケース
/// * `interval` - チェック間隔
/// * `tx` - ステータス結果を送信するチャンネル
async fn run_status_monitor(
    check_status: CheckServerStatus,
    interval: Duration,
    tx: mpsc::Sender<Vec<ServerStatus>>,
) {
    info!(interval = ?interval, "Starting status monitor");

    loop {
        let statuses = check_status.check_all().await;
        if tx.send(statuses).await.is_err() {
            break;
        }
        tokio::time::sleep(interval).await;
    }
}
