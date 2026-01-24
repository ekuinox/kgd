//! スレッドと Notion ページの紐付け情報を永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

/// 日報エントリの情報。
#[derive(Debug, Clone)]
pub struct DiaryEntry {
    /// Discord スレッド ID
    pub thread_id: u64,
    /// Notion ページ ID
    pub page_id: String,
    /// Notion ページ URL
    pub page_url: String,
    /// 日付 (YYYY-MM-DD 形式)
    pub date: String,
    /// 作成日時
    pub created_at: DateTime<Utc>,
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
        .bind(entry.thread_id as i64)
        .bind(&entry.page_id)
        .bind(&entry.page_url)
        .bind(&entry.date)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert diary entry")?;

        Ok(())
    }

    /// スレッド ID からエントリを取得する。
    pub async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>> {
        let row = sqlx::query(
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

        Ok(row.map(|r| DiaryEntry {
            thread_id: r.get::<i64, _>("thread_id") as u64,
            page_id: r.get("page_id"),
            page_url: r.get("page_url"),
            date: r.get("date"),
            created_at: r.get("created_at"),
        }))
    }

    /// 日付からエントリを取得する。
    pub async fn get_by_date(&self, date: &str) -> Result<Option<DiaryEntry>> {
        let row = sqlx::query(
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

        Ok(row.map(|r| DiaryEntry {
            thread_id: r.get::<i64, _>("thread_id") as u64,
            page_id: r.get("page_id"),
            page_url: r.get("page_url"),
            date: r.get("date"),
            created_at: r.get("created_at"),
        }))
    }
}
