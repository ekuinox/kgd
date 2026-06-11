//! 日報スレッドのライフサイクル（作成・再開・クローズ）を管理するユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use tracing::{info, warn};

use kgd_domain::{DiaryEntry, format_date_in_timezone, today_in_timezone};

use super::ports::{Clock, DiaryRepository, DiscordGateway, NotionApi};

mod close;
mod types;

pub use types::{
    CloseAndNewPrecheck, DiaryCloseOutcome, DiaryCreateOutcome, DiaryLifecycleSettings,
};

/// ボタン有無を確認する際に遡るメッセージ数。
const BUTTON_LOOKBACK_LIMIT: u8 = 10;

/// 日報スレッドのライフサイクルを管理するユースケース。
pub struct ManageDiaryLifecycle {
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// Notion API ポート
    notion: Arc<dyn NotionApi>,
    /// Discord ゲートウェイポート
    gateway: Arc<dyn DiscordGateway>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// ライフサイクル設定
    settings: DiaryLifecycleSettings,
}

impl ManageDiaryLifecycle {
    /// 新しい ManageDiaryLifecycle を作成する。
    pub fn new(
        repo: Arc<dyn DiaryRepository>,
        notion: Arc<dyn NotionApi>,
        gateway: Arc<dyn DiscordGateway>,
        clock: Arc<dyn Clock>,
        settings: DiaryLifecycleSettings,
    ) -> Self {
        Self {
            repo,
            notion,
            gateway,
            clock,
            settings,
        }
    }

    /// 今日の日報を作成する。既に存在する場合は再開を試みる。
    pub async fn create_or_reopen(&self) -> Result<DiaryCreateOutcome> {
        let timezone = &self.settings.timezone;
        let date = today_in_timezone(self.clock.now(), timezone);

        // 既に今日の日報が存在する場合は再開を試みる
        if let Some(entry) = self.repo.get_by_date(date).await? {
            let reopened = self.gateway.reopen_thread(entry.thread_id).await?;

            if let Err(error) = self.ensure_close_and_new_button(entry.thread_id).await {
                warn!(
                    error = %error,
                    thread_id = entry.thread_id,
                    "Failed to ensure close button on existing diary thread"
                );
            }

            info!(
                date = %date,
                thread_id = entry.thread_id,
                reopened,
                "Diary thread already exists for today"
            );

            return Ok(if reopened {
                DiaryCreateOutcome::Reopened {
                    thread_id: entry.thread_id,
                }
            } else {
                DiaryCreateOutcome::ExistsButNotReopened {
                    thread_id: entry.thread_id,
                }
            });
        }

        // 日付を文字列に変換 (YYYY-MM-DD 形式、設定されたタイムゾーンで表示)
        let date_str = format_date_in_timezone(date, timezone);

        let (page_id, page_url, reused_page) = self.ensure_page(&date_str).await?;

        // Discord フォーラムにスレッドを作成
        let thread_id = self
            .gateway
            .create_diary_forum_post(self.settings.forum_channel_id, &date_str, &page_url)
            .await
            .context("フォーラムスレッドの作成に失敗しました")?;

        self.ensure_close_and_new_button(thread_id).await?;

        let entry = DiaryEntry {
            thread_id,
            page_id,
            page_url: page_url.clone(),
            date,
            created_at: self.clock.now(),
        };
        self.repo.insert(&entry).await?;

        info!(date = %date, thread_id, reused = reused_page, "Diary created");

        Ok(DiaryCreateOutcome::Created {
            thread_id,
            page_url,
            reused_page,
        })
    }

    /// 既存の Notion ページを検索し、なければ新規作成する。
    ///
    /// # Returns
    /// (ページ ID, ページ URL, 既存ページを再利用したかどうか)
    async fn ensure_page(&self, date_str: &str) -> Result<(String, String, bool)> {
        if let Some((page_id, page_url)) = self
            .notion
            .find_diary_page_by_title(date_str)
            .await
            .context("Notion ページの検索に失敗しました")?
        {
            info!(page_id = %page_id, "Found existing Notion page");
            return Ok((page_id, page_url, true));
        }

        info!(title = %date_str, "Creating new Notion page");
        let (page_id, page_url) = self
            .notion
            .create_diary_page(date_str)
            .await
            .context("Notion ページの作成に失敗しました")?;
        Ok((page_id, page_url, false))
    }

    /// スレッドにクローズ & 新規作成ボタンがなければ送信する。
    async fn ensure_close_and_new_button(&self, thread_id: u64) -> Result<()> {
        if self
            .gateway
            .has_close_and_new_button(thread_id, BUTTON_LOOKBACK_LIMIT)
            .await?
        {
            return Ok(());
        }

        self.gateway.send_close_and_new_button(thread_id).await?;

        info!(thread_id, "Sent fallback close-and-new button message");

        Ok(())
    }
}

#[cfg(test)]
mod tests;
