//! 日報スレッドのクローズ・クローズ & 新規作成処理。

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use tracing::{info, warn};

use kgd_domain::DiaryEntry;

use super::{CloseAndNewPrecheck, DiaryCloseOutcome, ManageDiaryLifecycleUseCase};

impl ManageDiaryLifecycleUseCase {
    /// クローズ & 新規作成を実行してよいか確認する。
    ///
    /// 重い処理の前に応答を返す必要があるため、実処理 ([`Self::close_and_create_new`]) と分離している。
    pub async fn close_and_new_precheck(
        &self,
        current_channel_id: u64,
    ) -> Result<CloseAndNewPrecheck> {
        // 書き込み用チャンネル経由の場合は日報スレッドの登録チェックをスキップする
        let in_write_channel = self.settings.write_channel_id == current_channel_id;
        if !in_write_channel && self.repo.get_by_thread(current_channel_id).await?.is_none() {
            return Ok(CloseAndNewPrecheck::NotDiaryThread);
        }

        let target_date = self.new_diary_date();
        if let Some(today_entry) = self.repo.get_by_date(target_date).await? {
            return Ok(if today_entry.thread_id == current_channel_id {
                CloseAndNewPrecheck::AlreadyLatest
            } else {
                CloseAndNewPrecheck::LatestExists {
                    thread_id: today_entry.thread_id,
                }
            });
        }

        Ok(CloseAndNewPrecheck::ReadyToCreate)
    }

    /// 現在のスレッドをクローズし、今日の日報スレッドを新規作成する。
    ///
    /// 事前に [`Self::close_and_new_precheck`] で `ReadyToCreate` を確認してから呼ぶこと。
    pub async fn close_and_create_new(&self, current_channel_id: u64) -> Result<u64> {
        let target_date = self.new_diary_date();
        let date_str = self.settings.calendar.format(target_date);

        // クローズ対象のスレッドを決める
        // （書き込み用チャンネルの場合は最新の日報スレッド、日報スレッドの場合はそのスレッド）
        let in_write_channel = self.settings.write_channel_id == current_channel_id;
        let thread_to_close = if in_write_channel {
            self.repo
                .get_latest_entry()
                .await?
                .map(|entry| entry.thread_id)
        } else {
            Some(current_channel_id)
        };

        let (page_id, page_url, _) = self.ensure_page(&date_str).await?;

        let new_thread_id = self
            .gateway
            .create_diary_forum_post(self.settings.forum_channel_id, &date_str, &page_url)
            .await
            .context("Failed to create forum post")?;

        self.ensure_close_and_new_button(new_thread_id).await?;

        info!(
            thread_id = new_thread_id,
            page_id = %page_id,
            "Created new diary thread"
        );

        let new_entry = DiaryEntry {
            thread_id: new_thread_id,
            page_id,
            page_url,
            date: target_date,
            created_at: self.clock.now(),
        };
        self.repo.insert(&new_entry).await?;

        self.gateway
            .send_text(
                current_channel_id,
                &format!("新しい日報スレッドを作成しました: <#{}>", new_thread_id),
            )
            .await
            .context("Failed to send mention message")?;

        if let Some(thread_to_close) = thread_to_close {
            // 書き込み用チャンネル経由ではクローズ対象が既にアーカイブ済みのことがあるため、
            // 失敗は警告に留めて続行する
            if let Err(error) = self.gateway.close_thread(thread_to_close).await {
                warn!(
                    error = %error,
                    thread_id = thread_to_close,
                    "Failed to close thread after sending mention message"
                );
            }

            info!(
                old_thread_id = thread_to_close,
                new_thread_id, "Diary thread closed by button"
            );
        }

        Ok(new_thread_id)
    }

    /// 日報スレッドをクローズする。
    pub async fn close(&self, thread_id: u64) -> Result<DiaryCloseOutcome> {
        if self.repo.get_by_thread(thread_id).await?.is_none() {
            return Ok(DiaryCloseOutcome::NotDiaryThread);
        }

        self.gateway
            .close_thread(thread_id)
            .await
            .context("スレッドのクローズに失敗しました")?;

        info!(thread_id, "Diary thread closed");

        Ok(DiaryCloseOutcome::Closed)
    }

    /// クローズ & 新規作成で対象にする日報日を返す。
    ///
    /// 一日の始まりより前ならまだ始まっていない次の日報日を対象にし、
    /// 深夜のうちに翌日の日報を始められるようにする。
    fn new_diary_date(&self) -> DateTime<Utc> {
        let calendar = &self.settings.calendar;
        let now = self.clock.now();
        calendar
            .early_next_day(now)
            .unwrap_or_else(|| calendar.today(now))
    }
}
