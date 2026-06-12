//! 日報スレッドの走査と未同期メッセージの再同期処理。

use anyhow::{Context as _, Result};
use tracing::{error, info};

use kgd_domain::today_in_timezone;

use super::{DiaryThreadSyncReport, RunDiaryMaintenanceUseCase, THREAD_SYNC_BATCH_SIZE};

impl RunDiaryMaintenanceUseCase {
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
    pub(super) async fn sync_recent_threads(&self) -> Result<()> {
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
