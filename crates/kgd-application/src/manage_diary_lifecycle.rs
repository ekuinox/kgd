//! 日報スレッドのライフサイクル（作成・再開・クローズ）を管理するユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono_tz::Tz;
use tracing::{info, warn};

use kgd_domain::{DiaryEntry, format_date_in_timezone, today_in_timezone};

use super::ports::{Clock, DiaryRepository, DiscordGateway, NotionApi};

/// ボタン有無を確認する際に遡るメッセージ数。
const BUTTON_LOOKBACK_LIMIT: u8 = 10;

/// ライフサイクル管理の設定。
#[derive(Debug, Clone)]
pub struct DiaryLifecycleSettings {
    /// 日報の日付計算に使用するタイムゾーン
    pub timezone: Tz,
    /// 日報スレッドを作成するフォーラムチャンネル ID
    pub forum_channel_id: u64,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
}

/// 日報作成（/diary new）の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiaryCreateOutcome {
    /// 既存スレッドを再開した
    Reopened {
        /// 再開したスレッド ID
        thread_id: u64,
    },
    /// 既存スレッドがあるが再開できなかった
    ExistsButNotReopened {
        /// 既存スレッド ID
        thread_id: u64,
    },
    /// 新しくスレッドを作成した
    Created {
        /// 作成したスレッド ID
        thread_id: u64,
        /// Notion ページ URL
        page_url: String,
        /// 既存の Notion ページを再利用したかどうか
        reused_page: bool,
    },
}

/// クローズ & 新規作成の事前確認の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseAndNewPrecheck {
    /// 対象スレッドが日報スレッドではない
    NotDiaryThread,
    /// 対象スレッドが今日の最新の日報である
    AlreadyLatest,
    /// 今日の最新の日報が別に存在する
    LatestExists {
        /// 最新の日報スレッド ID
        thread_id: u64,
    },
    /// 新規作成へ進める
    ReadyToCreate,
}

/// 日報クローズ（/diary close）の結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiaryCloseOutcome {
    /// 対象スレッドが日報スレッドではない
    NotDiaryThread,
    /// クローズした
    Closed,
}

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

        let today = today_in_timezone(self.clock.now(), &self.settings.timezone);
        if let Some(today_entry) = self.repo.get_by_date(today).await? {
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
        let timezone = &self.settings.timezone;
        let today = today_in_timezone(self.clock.now(), timezone);
        let date_str = format_date_in_timezone(today, timezone);

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
            date: today,
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
mod tests {
    use chrono::{DateTime, TimeZone as _, Utc};
    use mockall::predicate::eq;

    use crate::ports::{MockClock, MockDiaryRepository, MockDiscordGateway, MockNotionApi};

    use super::*;

    /// テスト用に UTC の DateTime を作る。
    fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    /// テスト用の設定を作る。
    fn settings() -> DiaryLifecycleSettings {
        DiaryLifecycleSettings {
            timezone: chrono_tz::UTC,
            forum_channel_id: 555,
            write_channel_id: 500,
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

    /// 固定時刻を返す MockClock を作る。
    fn fixed_clock(now: DateTime<Utc>) -> MockClock {
        let mut clock = MockClock::new();
        clock.expect_now().returning(move || now);
        clock
    }

    fn lifecycle(
        repo: MockDiaryRepository,
        notion: MockNotionApi,
        gateway: MockDiscordGateway,
        clock: MockClock,
    ) -> ManageDiaryLifecycle {
        ManageDiaryLifecycle::new(
            Arc::new(repo),
            Arc::new(notion),
            Arc::new(gateway),
            Arc::new(clock),
            settings(),
        )
    }

    #[tokio::test]
    async fn create_reopens_existing_thread() {
        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date()
            .with(eq(today))
            .times(1)
            .returning(move |_| Ok(Some(entry(100, today))));
        let notion = MockNotionApi::new(); // Notion は呼ばれない
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_reopen_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(true));
        gateway
            .expect_has_close_and_new_button()
            .times(1)
            .returning(|_, _| Ok(true));
        gateway.expect_create_diary_forum_post().times(0);
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, notion, gateway, clock);
        let outcome = l.create_or_reopen().await.unwrap();

        assert_eq!(outcome, DiaryCreateOutcome::Reopened { thread_id: 100 });
    }

    #[tokio::test]
    async fn create_makes_new_thread_with_existing_page() {
        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date().times(1).returning(|_| Ok(None));
        repo.expect_insert()
            .withf(move |e| e.thread_id == 200 && e.page_id == "page-x" && e.date == today)
            .times(1)
            .returning(|_| Ok(()));
        let mut notion = MockNotionApi::new();
        // 既存ページが見つかる → create は呼ばれない
        notion
            .expect_find_diary_page_by_title()
            .with(eq("2025-01-02"))
            .times(1)
            .returning(|_| {
                Ok(Some((
                    "page-x".to_string(),
                    "https://notion.example/x".to_string(),
                )))
            });
        notion.expect_create_diary_page().times(0);
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_create_diary_forum_post()
            .with(eq(555u64), eq("2025-01-02"), eq("https://notion.example/x"))
            .times(1)
            .returning(|_, _, _| Ok(200));
        gateway
            .expect_has_close_and_new_button()
            .times(1)
            .returning(|_, _| Ok(true));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, notion, gateway, clock);
        let outcome = l.create_or_reopen().await.unwrap();

        assert_eq!(
            outcome,
            DiaryCreateOutcome::Created {
                thread_id: 200,
                page_url: "https://notion.example/x".to_string(),
                reused_page: true,
            }
        );
    }

    #[tokio::test]
    async fn create_makes_new_page_when_not_found() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_date().times(1).returning(|_| Ok(None));
        repo.expect_insert().times(1).returning(|_| Ok(()));
        let mut notion = MockNotionApi::new();
        notion
            .expect_find_diary_page_by_title()
            .times(1)
            .returning(|_| Ok(None));
        notion
            .expect_create_diary_page()
            .with(eq("2025-01-02"))
            .times(1)
            .returning(|_| {
                Ok((
                    "page-new".to_string(),
                    "https://notion.example/new".to_string(),
                ))
            });
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_create_diary_forum_post()
            .times(1)
            .returning(|_, _, _| Ok(201));
        // ボタンが無ければ送信する
        gateway
            .expect_has_close_and_new_button()
            .times(1)
            .returning(|_, _| Ok(false));
        gateway
            .expect_send_close_and_new_button()
            .with(eq(201u64))
            .times(1)
            .returning(|_| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, notion, gateway, clock);
        let outcome = l.create_or_reopen().await.unwrap();

        assert_eq!(
            outcome,
            DiaryCreateOutcome::Created {
                thread_id: 201,
                page_url: "https://notion.example/new".to_string(),
                reused_page: false,
            }
        );
    }

    #[tokio::test]
    async fn precheck_detects_non_diary_thread() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(None));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);
        let precheck = l.close_and_new_precheck(100).await.unwrap();

        assert_eq!(precheck, CloseAndNewPrecheck::NotDiaryThread);
    }

    #[tokio::test]
    async fn precheck_detects_latest_thread() {
        let today = utc(2025, 1, 2, 0, 0);
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_thread()
            .times(2)
            .returning(move |id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
        repo.expect_get_by_date()
            .times(2)
            .returning(move |_| Ok(Some(entry(300, today))));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);

        // 自分自身が最新
        assert_eq!(
            l.close_and_new_precheck(300).await.unwrap(),
            CloseAndNewPrecheck::AlreadyLatest
        );
        // 別スレッドが最新
        assert_eq!(
            l.close_and_new_precheck(100).await.unwrap(),
            CloseAndNewPrecheck::LatestExists { thread_id: 300 }
        );
    }

    #[tokio::test]
    async fn close_and_create_new_mentions_then_closes_old_thread() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_insert().times(1).returning(|_| Ok(()));
        let mut notion = MockNotionApi::new();
        notion
            .expect_find_diary_page_by_title()
            .times(1)
            .returning(|_| Ok(None));
        notion
            .expect_create_diary_page()
            .times(1)
            .returning(|_| Ok(("p".to_string(), "u".to_string())));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_create_diary_forum_post()
            .times(1)
            .returning(|_, _, _| Ok(400));
        gateway
            .expect_has_close_and_new_button()
            .times(1)
            .returning(|_, _| Ok(true));
        // 旧スレッドへの案内 → クローズの順
        gateway
            .expect_send_text()
            .withf(|channel_id, content| *channel_id == 100 && content.contains("<#400>"))
            .times(1)
            .returning(|_, _| Ok(901));
        gateway
            .expect_close_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, notion, gateway, clock);
        let new_thread_id = l.close_and_create_new(100).await.unwrap();

        assert_eq!(new_thread_id, 400);
    }

    #[tokio::test]
    async fn close_and_create_new_from_write_channel_closes_latest_thread() {
        let mut repo = MockDiaryRepository::new();
        // 書き込み用チャンネル経由では最新エントリのスレッドをクローズ対象にする
        repo.expect_get_latest_entry()
            .times(1)
            .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
        repo.expect_insert().times(1).returning(|_| Ok(()));
        let mut notion = MockNotionApi::new();
        notion
            .expect_find_diary_page_by_title()
            .times(1)
            .returning(|_| Ok(None));
        notion
            .expect_create_diary_page()
            .times(1)
            .returning(|_| Ok(("p".to_string(), "u".to_string())));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_create_diary_forum_post()
            .times(1)
            .returning(|_, _, _| Ok(400));
        gateway
            .expect_has_close_and_new_button()
            .times(1)
            .returning(|_, _| Ok(true));
        // 案内はボタンが押された書き込み用チャンネルへ送る
        gateway
            .expect_send_text()
            .withf(|channel_id, content| *channel_id == 500 && content.contains("<#400>"))
            .times(1)
            .returning(|_, _| Ok(902));
        gateway
            .expect_close_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, notion, gateway, clock);
        let new_thread_id = l.close_and_create_new(500).await.unwrap();

        assert_eq!(new_thread_id, 400);
    }

    #[tokio::test]
    async fn precheck_allows_write_channel_without_thread_registration() {
        let mut repo = MockDiaryRepository::new();
        // 書き込み用チャンネルでは get_by_thread を確認しない
        repo.expect_get_by_thread().times(0);
        repo.expect_get_by_date().times(1).returning(|_| Ok(None));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);
        let precheck = l.close_and_new_precheck(500).await.unwrap();

        assert_eq!(precheck, CloseAndNewPrecheck::ReadyToCreate);
    }

    #[tokio::test]
    async fn close_skips_non_diary_thread() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_thread().times(1).returning(|_| Ok(None));
        let mut gateway = MockDiscordGateway::new();
        gateway.expect_close_thread().times(0);
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, MockNotionApi::new(), gateway, clock);
        let outcome = l.close(100).await.unwrap();

        assert_eq!(outcome, DiaryCloseOutcome::NotDiaryThread);
    }

    #[tokio::test]
    async fn close_closes_diary_thread() {
        let mut repo = MockDiaryRepository::new();
        repo.expect_get_by_thread()
            .times(1)
            .returning(|id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
        let mut gateway = MockDiscordGateway::new();
        gateway
            .expect_close_thread()
            .with(eq(100u64))
            .times(1)
            .returning(|_| Ok(()));
        let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

        let l = lifecycle(repo, MockNotionApi::new(), gateway, clock);
        let outcome = l.close(100).await.unwrap();

        assert_eq!(outcome, DiaryCloseOutcome::Closed);
    }
}
