//! HTTP サーバーの常駐タスク。

use std::net::SocketAddr;

use anyhow::{Context as _, Result};
use axum::Router;
use tokio::net::TcpListener;
use tracing::info;

/// 指定アドレスで HTTP サーバーを起動し、終了するまで待つ。
pub async fn serve_http(listen: SocketAddr, router: Router) -> Result<()> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("Failed to bind {listen}"))?;

    info!(%listen, "HTTP server started");

    axum::serve(listener, router)
        .await
        .context("HTTP server error")
}
