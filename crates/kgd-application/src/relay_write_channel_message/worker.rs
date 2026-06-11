//! 書き込み用チャンネルイベントを直列処理する転記ワーカー。

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::error;

use kgd_domain::SyncMessage;

use super::RelayWriteChannelMessage;

/// 書き込み用チャンネルで起きたイベント。到着順にワーカーが処理する。
#[derive(Debug)]
pub enum WriteChannelEvent {
    /// メッセージが投稿された
    Posted(SyncMessage),
    /// メッセージが編集された
    Updated(SyncMessage),
    /// メッセージが削除された
    Deleted {
        /// 書き込み用チャンネルの元メッセージ ID
        source_message_id: u64,
    },
}

/// キューからイベントを到着順に 1 件ずつ取り出して転記処理を行うワーカー。
///
/// Discord のイベントハンドラは並行実行されるため、転記の順序
/// （スレッドへの投稿順・Notion のブロック順）を保証するためにここで直列化する。
/// このループは全送信側が閉じられるまで戻らないため、呼び出し側で spawn すること。
pub async fn run_relay_worker(
    relay: Arc<RelayWriteChannelMessage>,
    mut rx: mpsc::Receiver<WriteChannelEvent>,
) {
    while let Some(event) = rx.recv().await {
        let result = match &event {
            WriteChannelEvent::Posted(message) => relay.relay(message).await,
            WriteChannelEvent::Updated(message) => relay.update(message).await,
            WriteChannelEvent::Deleted { source_message_id } => {
                relay.delete(*source_message_id).await
            }
        };
        if let Err(error) = result {
            error!(error = %error, "Failed to process write channel event");
        }
    }
}
