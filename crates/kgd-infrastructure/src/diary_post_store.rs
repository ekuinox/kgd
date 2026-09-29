//! 日報への bot の投稿の進み具合を永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, PgPool};

use kgd_application::ports::DiaryPostRepository;
use kgd_domain::DiaryPostRecord;

/// 日報への投稿の進み具合を管理するストア。
#[derive(Clone)]
pub struct DiaryPostStore {
    pool: PgPool,
}

impl DiaryPostStore {
    /// 既存のプールからストアを作る。
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// diary_posts テーブルの行。
#[derive(Debug, Clone, FromRow)]
struct DiaryPostRow {
    /// 投稿のキー
    post_key: String,
    /// 載せる先の日報日
    diary_date: NaiveDate,
    /// Notion ページへ載せた時刻
    notion_posted_at: Option<DateTime<Utc>>,
    /// スレッドへ投稿したメッセージ ID
    thread_message_id: Option<i64>,
    /// スレッドへ投稿した時刻
    thread_posted_at: Option<DateTime<Utc>>,
    /// スキップした時刻
    skipped_at: Option<DateTime<Utc>>,
}

impl From<DiaryPostRow> for DiaryPostRecord {
    fn from(row: DiaryPostRow) -> Self {
        Self {
            key: row.post_key,
            diary_date: row.diary_date,
            notion_posted_at: row.notion_posted_at,
            thread_message_id: row.thread_message_id.map(|id| id as u64),
            thread_posted_at: row.thread_posted_at,
            skipped_at: row.skipped_at,
        }
    }
}

#[async_trait::async_trait]
impl DiaryPostRepository for DiaryPostStore {
    async fn get(&self, key: &str) -> Result<Option<DiaryPostRecord>> {
        let row: Option<DiaryPostRow> = sqlx::query_as(
            r#"
            SELECT post_key, diary_date, notion_posted_at, thread_message_id,
                   thread_posted_at, skipped_at
            FROM diary_posts
            WHERE post_key = $1
            "#,
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary post")?;

        Ok(row.map(DiaryPostRecord::from))
    }

    async fn mark_notion_posted(
        &self,
        key: &str,
        date: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, notion_posted_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (post_key) DO UPDATE SET notion_posted_at = EXCLUDED.notion_posted_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record notion post")?;
        Ok(())
    }

    async fn mark_thread_posted(
        &self,
        key: &str,
        date: NaiveDate,
        message_id: u64,
        at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, thread_message_id, thread_posted_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (post_key) DO UPDATE SET
                thread_message_id = EXCLUDED.thread_message_id,
                thread_posted_at = EXCLUDED.thread_posted_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(message_id as i64)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record thread post")?;
        Ok(())
    }

    async fn mark_skipped(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_posts (post_key, diary_date, skipped_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (post_key) DO UPDATE SET skipped_at = EXCLUDED.skipped_at
            "#,
        )
        .bind(key)
        .bind(date)
        .bind(at)
        .execute(&self.pool)
        .await
        .context("Failed to record skipped diary post")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone as _, Utc};

    use super::*;

    /// 行が記録へ変換され、メッセージ ID が u64 に戻ることを確認する。
    #[test]
    fn diary_post_row_converts_into_record() {
        let at = Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap();
        let row = DiaryPostRow {
            post_key: "location-report:2026-09-28".to_string(),
            diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            notion_posted_at: Some(at),
            thread_message_id: Some(1_234_567_890_123_456_789),
            thread_posted_at: Some(at),
            skipped_at: None,
        };

        let record = DiaryPostRecord::from(row);

        assert_eq!(record.key, "location-report:2026-09-28");
        assert_eq!(record.thread_message_id, Some(1_234_567_890_123_456_789));
        assert!(record.is_done());
    }
}
