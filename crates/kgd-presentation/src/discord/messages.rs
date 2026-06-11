//! メッセージ作成・編集・削除イベントの処理本体。

use serenity::{
    all::{ChannelId, ChannelType, Message, MessageUpdateEvent},
    client::Context as SerenityContext,
    model::id::MessageId,
};
use tracing::{error, info};

use kgd_application::WriteChannelEvent;
use kgd_infrastructure::to_sync_message;

use super::Handler;

impl Handler {
    /// メッセージ作成イベントを処理する。
    pub(crate) async fn on_message(&self, ctx: SerenityContext, message: Message) {
        // Bot 自身のメッセージは無視
        // 注意: write_channel 分岐より先に判定すること
        // （Bot が送る転記メッセージを処理対象にするとループする）
        if message.author.bot {
            return;
        }

        // 書き込み用チャンネルへの投稿は転記ワーカーのキューへ送る
        // （転記順を投稿順に保つため、処理自体は単一ワーカーが直列に行う）
        if self.settings.write_channel_id == message.channel_id.get() {
            let sync_message = to_sync_message(&message);
            if let Err(error) = self
                .relay_tx
                .send(WriteChannelEvent::Posted(sync_message))
                .await
            {
                error!(error = %error, "Failed to enqueue write channel message");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = message.channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(entry)) = self
            .diary_store
            .get_by_thread(message.channel_id.get())
            .await
        else {
            return;
        };
        let page_id = entry.page_id.clone();

        // Notion に同期（成功時はリアクションを付与する）
        let sync_message = to_sync_message(&message);
        match self
            .maintenance
            .sync_message_with_reaction(&page_id, &sync_message)
            .await
        {
            Ok((true, block_count)) => {
                info!(
                    thread_id = message.channel_id.get(),
                    message_id = message.id.get(),
                    blocks = block_count,
                    "Message synced to Notion"
                );
            }
            Ok(_) => {
                // スキップ (空メッセージなど)
            }
            Err(e) => {
                error!(error = %e, "Failed to sync message to Notion");
            }
        }
    }

    /// メッセージ編集イベントを処理する。
    pub(crate) async fn on_message_update(&self, ctx: SerenityContext, event: MessageUpdateEvent) {
        // Bot 自身のメッセージは無視
        if event.author.as_ref().is_some_and(|a| a.bot) {
            return;
        }

        // 書き込み用チャンネルでの編集は Notion ブロックと転記メッセージに反映する
        if self.settings.write_channel_id == event.channel_id.get() {
            if let Err(error) = self.handle_write_channel_update(&ctx, &event).await {
                error!(error = %error, "Failed to handle write channel message update");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = event.channel_id.to_channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(_entry)) = self.diary_store.get_by_thread(event.channel_id.get()).await else {
            return;
        };

        // コンテンツがない場合は無視
        let Some(content) = event.content else {
            return;
        };

        // メッセージを取得して更新
        let Ok(message) = event.channel_id.message(&ctx.http, event.id).await else {
            return;
        };

        // メッセージの内容で上書きして更新
        let mut sync_message = to_sync_message(&message);
        sync_message.content = content;

        match self.sync_service.update(&sync_message).await {
            Ok(true) => {
                info!(
                    thread_id = event.channel_id.get(),
                    message_id = event.id.get(),
                    "Message updated in Notion"
                );
            }
            Ok(false) => {
                // 対応するブロックがなかった（新規メッセージの可能性）
            }
            Err(e) => {
                error!(error = %e, "Failed to update message in Notion");
            }
        }
    }

    /// メッセージ削除イベントを処理する。
    pub(crate) async fn on_message_delete(
        &self,
        ctx: SerenityContext,
        channel_id: ChannelId,
        deleted_message_id: MessageId,
    ) {
        // 書き込み用チャンネルでの削除は転記ワーカーのキューへ送る
        if self.settings.write_channel_id == channel_id.get() {
            if let Err(error) = self
                .relay_tx
                .send(WriteChannelEvent::Deleted {
                    source_message_id: deleted_message_id.get(),
                })
                .await
            {
                error!(error = %error, "Failed to enqueue write channel message delete");
            }
            return;
        }

        // スレッドでない場合は無視
        let Ok(channel) = channel_id.to_channel(&ctx).await else {
            return;
        };
        let Some(guild_channel) = channel.guild() else {
            return;
        };
        if guild_channel.kind != ChannelType::PublicThread {
            return;
        }

        // 該当スレッドの日報エントリを取得
        let Ok(Some(_entry)) = self.diary_store.get_by_thread(channel_id.get()).await else {
            return;
        };

        // Notion から対応するブロックを削除
        match self.sync_service.delete(deleted_message_id.get()).await {
            Ok(true) => {
                info!(
                    thread_id = channel_id.get(),
                    message_id = deleted_message_id.get(),
                    "Message deleted from Notion"
                );
            }
            Ok(false) => {
                // 対応するブロックがなかった
            }
            Err(e) => {
                error!(error = %e, "Failed to delete message from Notion");
            }
        }
    }
}
