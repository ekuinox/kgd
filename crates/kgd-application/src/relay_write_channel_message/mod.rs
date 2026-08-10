//! 書き込み用チャンネルへの投稿を今日の日報スレッドへ転記するユースケース。

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use tracing::{error, info, warn};

use kgd_domain::{DiaryCalendar, RelayedMessage, SyncMessage, build_relay_content};

use super::{
    SyncDiaryMessageUseCase,
    ports::{Clock, DiaryRepository, DiscordGateway},
};

mod edits;
mod worker;

pub use worker::{WriteChannelEvent, run_relay_worker};

/// 転記ユースケースの設定。
#[derive(Debug, Clone)]
pub struct RelaySettings {
    /// 日報の日付計算に使用するカレンダー
    pub calendar: DiaryCalendar,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    pub sync_reaction: String,
}

/// 転記先が無いことを知らせるメッセージ。
///
/// 転記されなかった投稿を後から拾う仕組みは無いため、投稿し直しが必要なことまで伝える。
const NO_DIARY_NOTICE: &str = "今日の日報スレッドがまだ作成されていないため、この投稿は転記されません。日報作成ボタンで今日の日報を作成したうえで、投稿し直してください。";

/// 書き込み用チャンネルへの投稿を今日の日報スレッドへ転記するユースケース。
pub struct RelayWriteChannelMessageUseCase {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// メッセージ同期ユースケース
    sync: Arc<SyncDiaryMessageUseCase>,
    /// 転記設定
    settings: RelaySettings,
    /// 転記先が無いことを通知済みの日付。同じ日に何度も通知しないために保持する
    notified_missing_date: Mutex<Option<DateTime<Utc>>>,
}

impl RelayWriteChannelMessageUseCase {
    /// 新しい RelayWriteChannelMessageUseCase を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        sync: Arc<SyncDiaryMessageUseCase>,
        settings: RelaySettings,
    ) -> Self {
        Self {
            repo,
            gateway,
            clock,
            sync,
            settings,
            notified_missing_date: Mutex::new(None),
        }
    }

    /// 書き込み用チャンネルへの投稿を処理する。
    ///
    /// メッセージを今日の日報スレッドの Notion ページへ同期し、
    /// 同期に成功したらスレッドへ転記して対応を記録する。
    /// 同期がスキップされた場合（空メッセージなど）は転記も行わない。
    ///
    /// 今日の日報が無い場合は、過去の日報に紛れ込むのを避けるため転記せず、
    /// 書き込み用チャンネルへその旨を通知する。
    pub async fn relay(&self, message: &SyncMessage) -> Result<()> {
        let today = self.settings.calendar.today(self.clock.now());
        let Some(entry) = self.repo.get_by_date(today).await? else {
            warn!(
                message_id = message.message_id,
                date = %today,
                "No diary entry for today to relay write channel message"
            );
            self.notify_missing_diary(message.channel_id, today).await;
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

    /// 転記先が無いことを書き込み用チャンネルへ通知する。
    ///
    /// 投稿のたびに通知すると煩わしいため、同じ日付につき一度だけ送る。
    /// 通知自体の失敗は転記処理の失敗としては扱わない。
    async fn notify_missing_diary(&self, channel_id: u64, today: DateTime<Utc>) {
        {
            let notified = self.notified_missing_date.lock().expect("lock poisoned");
            if *notified == Some(today) {
                return;
            }
        }

        match self.gateway.send_text(channel_id, NO_DIARY_NOTICE).await {
            Ok(_) => {
                let mut notified = self.notified_missing_date.lock().expect("lock poisoned");
                *notified = Some(today);
            }
            Err(error) => {
                error!(error = %error, channel_id, "Failed to notify missing diary entry");
            }
        }
    }
}

#[cfg(test)]
mod tests;
