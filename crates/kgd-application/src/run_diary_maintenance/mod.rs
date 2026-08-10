//! 日報の定期メンテナンス（自動クローズ・毎時同期・未同期メッセージ走査）のユースケース。

use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use tokio::sync::Mutex;
use tracing::{error, info};

use kgd_domain::{
    DiaryHourlySyncSlot, HourlySyncDecision, SyncMessage, decide_hourly_sync,
    should_attempt_auto_close,
};

use super::{
    SyncDiaryMessageUseCase,
    ports::{Clock, DiaryRepository, DiscordGateway},
};

mod thread_scan;
mod types;

pub use types::{DiaryMaintenanceSettings, DiaryThreadSyncReport};

/// スレッド走査時に 1 回の API 呼び出しで取得するメッセージ数。
const THREAD_SYNC_BATCH_SIZE: u8 = 100;

/// 日報の定期メンテナンスを実行するユースケース。
pub struct RunDiaryMaintenanceUseCase {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// メッセージ同期ユースケース
    sync: Arc<SyncDiaryMessageUseCase>,
    /// メンテナンス設定
    settings: DiaryMaintenanceSettings,
    /// 自動クローズ通知を送信済みの日付（タイムゾーン基準）
    last_auto_close_notification_date: Mutex<Option<NaiveDate>>,
    /// 毎時同期を最後に試行した時間帯
    last_hourly_sync_slot: Mutex<Option<DiaryHourlySyncSlot>>,
}

impl RunDiaryMaintenanceUseCase {
    /// 新しい RunDiaryMaintenanceUseCase を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        sync: Arc<SyncDiaryMessageUseCase>,
        settings: DiaryMaintenanceSettings,
    ) -> Self {
        Self {
            repo,
            gateway,
            clock,
            sync,
            settings,
            last_auto_close_notification_date: Mutex::new(None),
            last_hourly_sync_slot: Mutex::new(None),
        }
    }

    /// 自動クローズのチェックを行い、必要ならボタン付きメッセージを送信する。
    pub async fn check_auto_close(&self) -> Result<()> {
        let calendar = &self.settings.calendar;
        let now = self.clock.now();
        let today = calendar.today(now);
        let today_local = calendar.local_date(now);

        // IO を呼ぶ前のゲート判定（機能無効・同じ日報日に通知済み）
        {
            let last_notified = *self.last_auto_close_notification_date.lock().await;
            if !should_attempt_auto_close(
                now,
                calendar,
                self.settings.auto_close_enabled,
                last_notified,
            ) {
                return Ok(());
            }
        }

        // 最新のエントリのみを取得
        let Some(entry) = self.repo.get_latest_entry().await? else {
            return Ok(());
        };

        // 最新エントリが今日以降なら通知は不要
        if entry.date >= today {
            return Ok(());
        }

        // スレッドがまだアクティブな場合のみスレッドへ送信する
        let thread_active = self
            .gateway
            .thread_state(entry.thread_id)
            .await?
            .is_some_and(|state| !state.is_closed());
        if thread_active {
            self.gateway
                .send_close_and_new_button(entry.thread_id)
                .await?;
            info!(thread_id = entry.thread_id, "Sent auto-close button");
        }

        // 書き込み用チャンネルにも送信する
        // （スレッドがアーカイブ済みでも、書き込み用からは新しい日報を作成できるようにする）
        let write_channel_id = self.settings.write_channel_id;
        self.gateway
            .send_write_channel_new_diary_button(write_channel_id)
            .await?;
        info!(
            channel_id = write_channel_id,
            "Sent new-diary button to write channel"
        );

        *self.last_auto_close_notification_date.lock().await = Some(today_local);

        Ok(())
    }

    /// 毎時の境目で直近 3 日分の日報スレッドを再同期する。
    ///
    /// 起動直後は現在の時間帯だけ記録し、次の時間帯に切り替わるまでは同期しない。
    pub async fn check_hourly_sync(&self) -> Result<()> {
        let current_slot =
            DiaryHourlySyncSlot::from(self.clock.now(), self.settings.calendar.timezone());

        {
            let mut last_hourly_sync_slot = self.last_hourly_sync_slot.lock().await;
            match decide_hourly_sync(*last_hourly_sync_slot, current_slot) {
                // 起動直後は現在の時間帯だけ記録し、次の時間帯から同期を始める。
                HourlySyncDecision::RecordOnly => {
                    *last_hourly_sync_slot = Some(current_slot);
                    return Ok(());
                }
                HourlySyncDecision::Skip => return Ok(()),
                HourlySyncDecision::Sync => {}
            }
        }

        self.sync_recent_threads().await?;
        *self.last_hourly_sync_slot.lock().await = Some(current_slot);

        info!("Hourly diary sync finished");

        Ok(())
    }

    /// 1 件の日報メッセージを Notion に同期し、成功時は同期済みリアクションを付与する。
    ///
    /// # Returns
    /// (同期されたかどうか, 作成されたブロック数)
    pub async fn sync_message_with_reaction(
        &self,
        page_id: &str,
        message: &SyncMessage,
    ) -> Result<(bool, usize)> {
        let result = self.sync.sync(page_id, message).await?;

        if !result.synced {
            return Ok((false, result.block_count));
        }

        // リアクション付与の失敗は同期自体の失敗とは扱わない
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

        Ok((true, result.block_count))
    }
}

#[cfg(test)]
mod tests;
