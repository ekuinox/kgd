//! スレッドと Notion ページの紐付け情報を永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

/// 日報エントリの情報。
#[derive(Debug, Clone)]
pub struct DiaryEntry {
    /// Discord スレッド ID
    pub thread_id: u64,
    /// Notion ページ ID
    pub page_id: String,
    /// Notion ページ URL
    pub page_url: String,
    /// 日付
    pub date: NaiveDate,
    /// 作成日時
    pub created_at: DateTime<Utc>,
}

/// データベース行との相互変換用の内部構造体。
#[derive(FromRow)]
struct DiaryEntryRow {
    thread_id: i64,
    page_id: String,
    page_url: String,
    date: NaiveDate,
    created_at: DateTime<Utc>,
}

impl From<DiaryEntryRow> for DiaryEntry {
    fn from(row: DiaryEntryRow) -> Self {
        Self {
            thread_id: row.thread_id as u64,
            page_id: row.page_id,
            page_url: row.page_url,
            date: row.date,
            created_at: row.created_at,
        }
    }
}

impl From<&DiaryEntry> for DiaryEntryRow {
    fn from(entry: &DiaryEntry) -> Self {
        Self {
            thread_id: entry.thread_id as i64,
            page_id: entry.page_id.clone(),
            page_url: entry.page_url.clone(),
            date: entry.date,
            created_at: entry.created_at,
        }
    }
}

/// スレッドと Notion ページの紐付け情報を管理するストア。
#[derive(Clone)]
pub struct DiaryStore {
    pool: PgPool,
}

impl DiaryStore {
    /// データベースに接続し、マイグレーションを実行する。
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .context("Failed to connect to database")?;

        // マイグレーションを実行
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("Failed to run migrations")?;

        Ok(Self { pool })
    }

    /// エントリを追加する。
    pub async fn insert(&self, entry: &DiaryEntry) -> Result<()> {
        let row = DiaryEntryRow::from(entry);
        sqlx::query(
            r#"
            INSERT INTO diary_entries (thread_id, page_id, page_url, date, created_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (thread_id) DO UPDATE SET
                page_id = EXCLUDED.page_id,
                page_url = EXCLUDED.page_url,
                date = EXCLUDED.date
            "#,
        )
        .bind(row.thread_id)
        .bind(&row.page_id)
        .bind(&row.page_url)
        .bind(row.date)
        .bind(row.created_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert diary entry")?;

        Ok(())
    }

    /// スレッド ID からエントリを取得する。
    pub async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>> {
        let row: Option<DiaryEntryRow> = sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            WHERE thread_id = $1
            "#,
        )
        .bind(thread_id as i64)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary entry by thread")?;

        Ok(row.map(DiaryEntry::from))
    }

    /// 日付からエントリを取得する。
    pub async fn get_by_date(&self, date: NaiveDate) -> Result<Option<DiaryEntry>> {
        let row: Option<DiaryEntryRow> = sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            WHERE date = $1
            "#,
        )
        .bind(date)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary entry by date")?;

        Ok(row.map(DiaryEntry::from))
    }
}
