//! 日報の定期メンテナンス（自動クローズ・毎時同期・未同期メッセージ走査）のユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::NaiveDate;
use chrono_tz::Tz;
use tokio::sync::Mutex;
use tracing::{error, info};

use kgd_domain::{
    DiaryHourlySyncSlot, HourlySyncDecision, SyncMessage, decide_hourly_sync,
    should_attempt_auto_close, should_send_auto_close, today_in_timezone,
};

use super::{
    SyncDiaryMessage,
    ports::{Clock, DiaryRepository, DiscordGateway},
};

/// スレッド走査時に 1 回の API 呼び出しで取得するメッセージ数。
const THREAD_SYNC_BATCH_SIZE: u8 = 100;

/// 定期メンテナンスの設定。
#[derive(Debug, Clone)]
pub struct DiaryMaintenanceSettings {
    /// 日報の日付計算に使用するタイムゾーン
    pub timezone: Tz,
    /// 自動クローズ機能を有効にするか
    pub auto_close_enabled: bool,
    /// 自動クローズの確認メッセージを送信する時刻（時）
    pub auto_close_hour: u32,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    pub sync_reaction: String,
}

/// 日報スレッド走査の結果レポート。
#[derive(Debug, Clone, Copy, Default)]
pub struct DiaryThreadSyncReport {
    /// 確認したメッセージ数
    pub checked_messages: usize,
    /// 新規に同期したメッセージ数
    pub synced_messages: usize,
    /// 既に同期済みだったメッセージ数
    pub already_synced_messages: usize,
    /// スキップしたメッセージ数
    pub skipped_messages: usize,
}

/// 日報の定期メンテナンスを実行するユースケース。
pub struct RunDiaryMaintenance {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// メッセージ同期ユースケース
    sync: Arc<SyncDiaryMessage>,
    /// メンテナンス設定
    settings: DiaryMaintenanceSettings,
    /// 自動クローズ通知を送信済みの日付（タイムゾーン基準）
    last_auto_close_notification_date: Mutex<Option<NaiveDate>>,
    /// 毎時同期を最後に試行した時間帯
    last_hourly_sync_slot: Mutex<Option<DiaryHourlySyncSlot>>,
}

impl RunDiaryMaintenance {
    /// 新しい RunDiaryMaintenance を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        sync: Arc<SyncDiaryMessage>,
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
        let timezone = &self.settings.timezone;
        let now = self.clock.now();
        let today = today_in_timezone(now, timezone);
        let today_local = now.with_timezone(timezone).date_naive();

        // IO を呼ぶ前のゲート判定（機能無効・指定時刻前・本日通知済み）
        {
            let last_notified = *self.last_auto_close_notification_date.lock().await;
            if !should_attempt_auto_close(
                now,
                timezone,
                self.settings.auto_close_enabled,
                self.settings.auto_close_hour,
                last_notified,
            ) {
                return Ok(());
            }
        }

        // 最新のエントリのみを取得
        let Some(entry) = self.repo.get_latest_entry().await? else {
            return Ok(());
        };

        // スレッドがまだアクティブかチェック
        let Some(thread_state) = self.gateway.thread_state(entry.thread_id).await? else {
            return Ok(());
        };

        // IO 取得後の最終判定（エントリ日付・スレッド状態）
        if !should_send_auto_close(Some(entry.date), today, thread_state.is_closed()) {
            return Ok(());
        }

        // ボタン付きメッセージを送信
        self.gateway
            .send_close_and_new_button(entry.thread_id)
            .await?;
        *self.last_auto_close_notification_date.lock().await = Some(today_local);

        info!(thread_id = entry.thread_id, "Sent auto-close button");

        Ok(())
    }

    /// 毎時の境目で直近 3 日分の日報スレッドを再同期する。
    ///
    /// 起動直後は現在の時間帯だけ記録し、次の時間帯に切り替わるまでは同期しない。
    pub async fn check_hourly_sync(&self) -> Result<()> {
        let current_slot = DiaryHourlySyncSlot::from(self.clock.now(), &self.settings.timezone);

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

    /// 指定した日報スレッドを全走査し、未同期メッセージだけを Notion に再同期する。
    ///
    /// 同期対象の期間絞り込みはここで持たず、呼び出し側で決める。
    pub async fn sync_missing_in_thread(&self, thread_id: u64) -> Result<DiaryThreadSyncReport> {
        let Some(entry) = self.repo.get_by_thread(thread_id).await? else {
            anyhow::bail!("Diary entry not found for thread {}", thread_id);
        };

        let Some(thread_state) = self.gateway.thread_state(thread_id).await? else {
            anyhow::bail!("Failed to fetch thread {}", thread_id);
        };
        if !thread_state.is_public_thread {
            anyhow::bail!("Channel {} is not a public thread", thread_id);
        }

        let mut before = None;
        let mut pending_messages = Vec::new();
        let mut report = DiaryThreadSyncReport::default();

        loop {
            let messages = self
                .gateway
                .fetch_messages_before(thread_id, before, THREAD_SYNC_BATCH_SIZE)
                .await
                .with_context(|| format!("Failed to fetch messages for thread {}", thread_id))?;
            if messages.is_empty() {
                break;
            }

            before = messages.last().map(|message| message.message_id);

            for message in messages {
                if message.is_bot {
                    continue;
                }

                pending_messages.push(message);
            }
        }

        pending_messages.reverse();
        report.checked_messages = pending_messages.len();

        for message in pending_messages {
            if self.repo.has_blocks_by_message(message.message_id).await? {
                report.already_synced_messages += 1;
                continue;
            }

            let (synced, _) = self
                .sync_message_with_reaction(&entry.page_id, &message)
                .await
                .with_context(|| {
                    format!(
                        "Failed to sync missing message {} in thread {}",
                        message.message_id, thread_id
                    )
                })?;

            if synced {
                report.synced_messages += 1;
            } else {
                report.skipped_messages += 1;
            }
        }

        info!(
            thread_id,
            checked_messages = report.checked_messages,
            synced_messages = report.synced_messages,
            already_synced_messages = report.already_synced_messages,
            skipped_messages = report.skipped_messages,
            "Checked diary thread for missing message sync"
        );

        Ok(report)
    }

    /// 直近 3 日分の日報スレッドを順番に再同期する。
    async fn sync_recent_threads(&self) -> Result<()> {
        let today = today_in_timezone(self.clock.now(), &self.settings.timezone);
        // 当日を含めた 3 日分だけを定期同期の対象にする。
        let start_date = today - chrono::Duration::days(2);
        let entries = self
            .repo
            .get_entries_in_date_range(start_date, today)
            .await?;

        let mut total = DiaryThreadSyncReport::default();

        for entry in entries {
            match self.sync_missing_in_thread(entry.thread_id).await {
                Ok(report) => {
                    total.checked_messages += report.checked_messages;
                    total.synced_messages += report.synced_messages;
                    total.already_synced_messages += report.already_synced_messages;
                    total.skipped_messages += report.skipped_messages;
                }
                Err(error) => {
                    error!(
                        error = %error,
                        thread_id = entry.thread_id,
                        "Failed to sync recent diary thread"
                    );
                }
            }
        }

        info!(
            start_date = %start_date,
            end_date = %today,
            checked_messages = total.checked_messages,
            synced_messages = total.synced_messages,
            already_synced_messages = total.already_synced_messages,
            skipped_messages = total.skipped_messages,
            "Recent diary sync finished"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use chrono::{DateTime, TimeZone as _, Utc};
    use mockall::Sequence;
    use mockall::predicate::eq;

    use kgd_domain::{DiaryEntry, ThreadState, compile_url_rules};

    use crate::ports::{
        MockAttachmentDownloader, MockClock, MockDiaryRepository, MockDiscordGateway,
        MockImageConverter, MockNotionApi,
    };

    use super::*;

    /// テスト用に UTC の DateTime を作る。
    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    /// テスト用の設定を作る。
    fn settings() -> DiaryMaintenanceSettings {
        DiaryMaintenanceSettings {
            timezone: chrono_tz::UTC,
            auto_close_enabled: true,
            auto_close_hour: 8,
            sync_reaction: "✅".to_string(),
        }
    }

    /// テスト用の日報エントリを作る。
    fn entry(thread_id: u64, date: DateTime<Utc>) -> DiaryEntry {
        DiaryEntry {
            thread_id,
            page_id: format!("page-{}", thread_id),
            page_url: format!("https://notion.example/{}", thread_id),
            date,
            created_at: date,
        }
    }

    /// 同期処理を伴わないテスト向けに空のモックで SyncDiaryMessage を作る。
    fn empty_sync_service() -> Arc<SyncDiaryMessage> {
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        Arc::new(SyncDiaryMessage::new(
            Arc::new(MockNotionApi::new()),
            Arc::new(MockDiaryRepository::new()),
            Arc::new(MockAttachmentDownloader::new()),
            Arc::new(MockImageConverter::new()),
            None,
            url_rules,
        ))
    }

    /// 固定時刻を返す MockClock を作る。
    fn fixed_clock(now: DateTime<Utc>) -> MockClock {
        let mut clock = MockClock::new();
        clock.expect_now().returning(move || now);
        clock
    }

    fn maintenance(
        repo: MockDiaryRepository,
        gateway: MockDiscordGateway,
        clock: MockClock,
        sync: Arc<SyncDiaryMessage>,
    ) -> RunDiaryMaintenance {
        RunDiaryMaintenance::new(
            Arc::new(repo),
            Arc::new(gateway),
            Arc::new(clock),
            sync,
            settings(),
        )
    }

    #[tokio::test]
    async fn auto_close_skips_before_hour() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_latest_entry().times(0);
        let gateway = MockDiscordGateway::new();
        // 08:00 より前なので何もしない
        let clock = fixed_clock(utc(2025, 1, 2, 7, 0));

        let m = maintenance(repo, gateway, clock, empty_sync_service());
        m.check_auto_close().await.unwrap();
    }

    #[tokio::test]
    async fn auto_close_sends_button_for_stale_active_thread() {
        let mut repo = MockDiaryRepository::new();
        // 最新エントリは前日のもの
        repo.expect_get_latest_entry()
            .times(1)
            .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_thread_state()
            .with(eq(100u64))
            .times(1)
            .returning(|_| {
                Ok(Some(ThreadState {
                    is_public_thread: true,
                    archived: false,
                    locked: false,
                }))
            });
        gateway
            .expect_send_close_and_new_button()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let m = maintenance(repo, gateway, clock, empty_sync_service());
        m.check_auto_close().await.unwrap();

        // 同じ日のうちは再送しない（repo への 2 回目の問い合わせ自体が発生しない）
        m.check_auto_close().await.unwrap();
    }

    #[tokio::test]
    async fn auto_close_skips_archived_thread() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_latest_entry()
            .times(1)
            .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
        let mut gateway = MockDiscordGateway::new();
        gateway.expect_thread_state().times(1).returning(|_| {
            Ok(Some(ThreadState {
                is_public_thread: true,
                archived: true,
                locked: false,
            }))
        });
        gateway.expect_send_close_and_new_button().times(0);
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let m = maintenance(repo, gateway, clock, empty_sync_service());
        m.check_auto_close().await.unwrap();
    }

    #[tokio::test]
    async fn hourly_sync_records_only_on_first_call_then_syncs_on_slot_change() {
        let mut repo = MockDiaryRepository::new();
        // スロット切り替え後の 1 回だけ同期が走る
        repo.expect_get_entries_in_date_range()
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let gateway = MockDiscordGateway::new();

        // 1 回目: 10 時（記録のみ）、2 回目: 10 時（同一スロット）、3 回目以降: 11 時（同期）
        let call_count = AtomicUsize::new(0);
        let mut clock = MockClock::new();
        clock.expect_now().returning(move || {
            let i = call_count.fetch_add(1, Ordering::SeqCst);
            if i < 2 {
                utc(2025, 1, 1, 10, 0)
            } else {
                utc(2025, 1, 1, 11, 0)
            }
        });

        let m = maintenance(repo, gateway, clock, empty_sync_service());
        m.check_hourly_sync().await.unwrap(); // 記録のみ
        m.check_hourly_sync().await.unwrap(); // 同一スロット: skip
        m.check_hourly_sync().await.unwrap(); // スロット変化: sync
    }

    #[tokio::test]
    async fn sync_missing_pages_through_thread_and_counts() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
        // メッセージ 1 は同期済み、メッセージ 2 は未同期（空コンテンツでスキップされる）
        repo.expect_has_blocks_by_message()
            .with(eq(1u64))
            .times(1)
            .returning(|_| Ok(true));
        repo.expect_has_blocks_by_message()
            .with(eq(2u64))
            .times(1)
            .returning(|_| Ok(false));

        let mut gateway = MockDiscordGateway::new();
        gateway.expect_thread_state().times(1).returning(|_| {
            Ok(Some(ThreadState {
                is_public_thread: true,
                archived: false,
                locked: false,
            }))
        });

        // ページング: 1 回目は 3 件（うち 1 件は bot）、2 回目は空
        let mut seq = Sequence::new();
        gateway
            .expect_fetch_messages_before()
            .with(eq(100u64), eq(None), eq(THREAD_SYNC_BATCH_SIZE))
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| {
                Ok(vec![
                    SyncMessage {
                        message_id: 2,
                        channel_id: 100,
                        content: String::new(),
                        is_bot: false,
                        attachments: vec![],
                    },
                    SyncMessage {
                        message_id: 9,
                        channel_id: 100,
                        content: "bot message".to_string(),
                        is_bot: true,
                        attachments: vec![],
                    },
                    SyncMessage {
                        message_id: 1,
                        channel_id: 100,
                        content: "synced".to_string(),
                        is_bot: false,
                        attachments: vec![],
                    },
                ])
            });
        gateway
            .expect_fetch_messages_before()
            .with(eq(100u64), eq(Some(1u64)), eq(THREAD_SYNC_BATCH_SIZE))
            .times(1)
            .in_sequence(&mut seq)
            .returning(|_, _, _| Ok(vec![]));

        let clock = fixed_clock(utc(2025, 1, 1, 12, 0));

        let m = maintenance(repo, gateway, clock, empty_sync_service());
        let report = m.sync_missing_in_thread(100).await.unwrap();

        // bot を除く 2 件を確認し、1 件は同期済み・1 件は空メッセージでスキップ
        assert_eq!(report.checked_messages, 2);
        assert_eq!(report.already_synced_messages, 1);
        assert_eq!(report.synced_messages, 0);
        assert_eq!(report.skipped_messages, 1);
    }

    #[tokio::test]
    async fn sync_message_with_reaction_adds_reaction_on_success() {
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        let mut notion = MockNotionApi::new();
        notion
            .expect_append_blocks()
            .times(1)
            .returning(|_, _| Ok(vec!["b1".to_string()]));
        let mut sync_repo = MockDiaryRepository::new();
        sync_repo
            .expect_insert_message_block()
            .times(1)
            .returning(|_| Ok(()));
        let sync = Arc::new(SyncDiaryMessage::new(
            Arc::new(notion),
            Arc::new(sync_repo),
            Arc::new(MockAttachmentDownloader::new()),
            Arc::new(MockImageConverter::new()),
            None,
            url_rules,
        ));

        let repo = MockDiaryRepository::new();
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_add_reaction()
            .with(eq(100u64), eq(10u64), eq("✅"))
            .times(1)
            .returning(|_, _, _| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 1, 12, 0));

        let m = maintenance(repo, gateway, clock, sync);
        let message = SyncMessage {
            message_id: 10,
            channel_id: 100,
            content: "hello".to_string(),
            is_bot: false,
            attachments: vec![],
        };
        let (synced, blocks) = m
            .sync_message_with_reaction("page-100", &message)
            .await
            .unwrap();

        assert!(synced);
        assert_eq!(blocks, 1);
    }
}
