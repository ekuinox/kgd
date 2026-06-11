//! サーバーステータスの Discord 通知。

use std::{sync::Arc, time::Duration};

use serenity::all::{ChannelId, CreateMessage, Http};
use tokio::sync::mpsc;
use tracing::error;

use kgd_domain::ServerStatus;

use crate::presenter::{present_server_status, render_embed};

/// サーバーステータスをDiscordチャンネルに通知するための構造体。
pub struct StatusNotifier {
    /// Discord API クライアント
    http: Arc<Http>,
    /// 通知先チャンネルID
    channel_id: ChannelId,
    /// ステータスチェック間隔（フッター表示用）
    interval: Duration,
}

impl StatusNotifier {
    /// 新しい StatusNotifier を作成する。
    pub fn new(http: Arc<Http>, channel_id: ChannelId, interval: Duration) -> Self {
        Self {
            http,
            channel_id,
            interval,
        }
    }

    /// サーバーステータスをDiscordチャンネルに埋め込みメッセージとして送信する。
    pub async fn send(&self, statuses: &[ServerStatus]) {
        let embed = render_embed(&present_server_status(statuses, self.interval));

        let message = CreateMessage::new().embed(embed);
        if let Err(e) = self.channel_id.send_message(&self.http, message).await {
            error!(error = %e, "Failed to send status message");
        }
    }
}

/// ステータスモニターからの通知を受信し、Discordに転送するループを実行する。
pub async fn run_status_receiver(
    notifier: StatusNotifier,
    mut rx: mpsc::Receiver<Vec<ServerStatus>>,
) {
    while let Some(statuses) = rx.recv().await {
        notifier.send(&statuses).await;
    }
}
