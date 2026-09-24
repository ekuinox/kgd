//! HTTP サーバーの常駐タスク。

use std::net::SocketAddr;

use anyhow::{Context as _, Result};
use axum::Router;
use tokio::net::TcpListener;
use tracing::info;

/// 指定アドレスで待ち受けソケットを用意する。
///
/// bind はここで同期的に行う。呼び出し側が起動処理の中で `?` を使って
/// 失敗を伝搬できるようにするため。ポート衝突などの bind 失敗を
/// `tokio::spawn` の中に隠すと、起動処理からは成功したように見えてしまう。
pub async fn bind_http(listen: SocketAddr) -> Result<TcpListener> {
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("Failed to bind {listen}"))?;

    info!(%listen, "HTTP server started");

    Ok(listener)
}

/// 待ち受けソケットで HTTP サーバーを起動し、終了するまで待つ。
///
/// bind は [`bind_http`] で済んでいる前提。ここで発生するのは接続を
/// 受け付け始めた後の実行時エラーのみ。
pub async fn serve_http(listener: TcpListener, router: Router) -> Result<()> {
    axum::serve(listener, router)
        .await
        .context("HTTP server error")
}
