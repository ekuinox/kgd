//! 書き込み用チャンネルへの投稿を最新の日報スレッドへ転記するユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono_tz::Tz;
use tracing::{error, info, warn};

use kgd_domain::{DiaryEntry, RelayedMessage, SyncMessage, build_relay_content, today_in_timezone};

use super::{
    SyncDiaryMessage,
    ports::{Clock, DiaryRepository, DiscordGateway},
};

mod edits;
mod worker;

pub use worker::{WriteChannelEvent, run_relay_worker};

/// 転記ユースケースの設定。
#[derive(Debug, Clone)]
pub struct RelaySettings {
    /// 日報の日付計算に使用するタイムゾーン
    pub timezone: Tz,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    pub sync_reaction: String,
}

/// 書き込み用チャンネルへの投稿を最新の日報スレッドへ転記するユースケース。
pub struct RelayWriteChannelMessage {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// メッセージ同期ユースケース
    sync: Arc<SyncDiaryMessage>,
    /// 転記設定
    settings: RelaySettings,
}

impl RelayWriteChannelMessage {
    /// 新しい RelayWriteChannelMessage を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        sync: Arc<SyncDiaryMessage>,
        settings: RelaySettings,
    ) -> Self {
        Self {
            repo,
            gateway,
            clock,
            sync,
            settings,
        }
    }

    /// 書き込み用チャンネルへの投稿を処理する。
    ///
    /// メッセージを最新の日報スレッドの Notion ページへ同期し、
    /// 同期に成功したらスレッドへ転記して対応を記録する。
    /// 同期がスキップされた場合（空メッセージなど）は転記も行わない。
    pub async fn relay(&self, message: &SyncMessage) -> Result<()> {
        let Some(entry) = self.latest_diary_entry_for_relay().await? else {
            warn!(
                message_id = message.message_id,
                "No diary entry found to relay write channel message"
            );
            return Ok(());
        };

        let result = self.sync.sync(&entry.page_id, message).await?;
        if !result.synced {
            // 同期対象がない場合は転記もしない
            return Ok(());
        }
        info!(
            thread_id = entry.thread_id,
            message_id = message.message_id,
            blocks = result.block_count,
            "Write channel message synced to Notion"
        );

        // 転記に失敗した場合は Notion ブロックを巻き戻して整合を保つ
        // （対応レコードが無いまま Notion にブロックが残ると、後続の編集・削除が追従できないため）
        let content = build_relay_content(message);
        let relayed_message_id = match self.gateway.send_text(entry.thread_id, &content).await {
            Ok(relayed_message_id) => relayed_message_id,
            Err(error) => {
                if let Err(rollback_error) = self.sync.delete(message.message_id).await {
                    error!(
                        error = %rollback_error,
                        message_id = message.message_id,
                        "Failed to roll back Notion blocks after relay failure"
                    );
                }
                return Err(error).context("Failed to relay message to diary thread");
            }
        };

        self.repo
            .upsert_relayed_message(&RelayedMessage {
                source_message_id: message.message_id,
                thread_id: entry.thread_id,
                relayed_message_id,
            })
            .await?;

        // すべて成功したらリアクションを付ける（失敗は同期の失敗とは扱わない）
        if let Err(error) = self
            .gateway
            .add_reaction(
                message.channel_id,
                message.message_id,
                &self.settings.sync_reaction,
            )
            .await
        {
            error!(error = %error, "Failed to add sync reaction");
        }

        info!(
            thread_id = entry.thread_id,
            message_id = message.message_id,
            relayed_message_id,
            "Write channel message relayed to diary thread"
        );

        Ok(())
    }

    /// 転記先となる最新の日報エントリを取得する。
    ///
    /// 今日の日報があればそれを、無ければ最新のエントリを返す。
    async fn latest_diary_entry_for_relay(&self) -> Result<Option<DiaryEntry>> {
        let today = today_in_timezone(self.clock.now(), &self.settings.timezone);
        if let Some(entry) = self.repo.get_by_date(today).await? {
            return Ok(Some(entry));
        }
        self.repo.get_latest_entry().await
    }
}

#[cfg(test)]
mod tests;
