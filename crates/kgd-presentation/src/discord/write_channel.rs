//! 書き込み用チャンネルのイベントを転記ユースケースへ引き渡す処理。

use anyhow::{Context as _, Result};
use serenity::{all::MessageUpdateEvent, client::Context as SerenityContext};

use kgd_application::WriteChannelEvent;
use kgd_infrastructure::to_sync_message;

use super::Handler;

impl Handler {
    /// 書き込み用チャンネルでのメッセージ編集を転記ユースケースへ引き渡す。
    pub(crate) async fn handle_write_channel_update(
        &self,
        ctx: &SerenityContext,
        event: &MessageUpdateEvent,
    ) -> Result<()> {
        // コンテンツがない場合は無視
        // （埋め込み展開などユーザー編集以外の更新イベントを除外する）
        if event.content.is_none() {
            return Ok(());
        }

        // 編集後の内容を取得
        let message = event
            .channel_id
            .message(&ctx.http, event.id)
            .await
            .context("Failed to fetch updated write channel message")?;

        // REST で取得したメッセージは guild_id を持たないためイベント側から補う
        let mut sync_message = to_sync_message(&message);
        sync_message.guild_id = event.guild_id.map(|guild_id| guild_id.get());

        self.relay_tx
            .send(WriteChannelEvent::Updated(sync_message))
            .await
            .context("Failed to enqueue write channel message update")
    }
}
