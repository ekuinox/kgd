//! スレッドと Notion ページの紐付け情報を永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

/// メッセージとブロックの対応情報。
#[derive(Debug, Clone, FromRow)]
pub struct MessageBlock {
    /// Discord メッセージ ID
    #[sqlx(try_from = "i64")]
    pub message_id: u64,
    /// Notion ブロック ID
    pub block_id: String,
    /// ブロックの種類
    pub block_type: String,
    /// ブロックの順序
    pub block_order: i32,
}

/// 日報エントリの情報。
#[derive(Debug, Clone, FromRow)]
pub struct DiaryEntry {
    /// Discord スレッド ID
    #[sqlx(try_from = "i64")]
    pub thread_id: u64,
    /// Notion ページ ID
    pub page_id: String,
    /// Notion ページ URL
    pub page_url: String,
    /// 日付
    pub date: DateTime<Utc>,
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
        .bind(entry.date)
        .bind(entry.created_at)
        .execute(&self.pool)
        .await
        .context("Failed to insert diary entry")?;

        Ok(())
    }

    /// スレッド ID からエントリを取得する。
    pub async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>> {
        sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            WHERE thread_id = $1
            "#,
        )
        .bind(thread_id as i64)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary entry by thread")
    }

    /// 日付からエントリを取得する。
    ///
    /// 指定された日時が含まれる日（その日の00:00:00から翌日の00:00:00まで）のエントリを検索する。
    pub async fn get_by_date(&self, date: DateTime<Utc>) -> Result<Option<DiaryEntry>> {
        sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            WHERE date = $1
            "#,
        )
        .bind(date)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch diary entry by date")
    }

    /// メッセージとブロックの対応を保存する。
    pub async fn insert_message_block(&self, block: &MessageBlock) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_message_blocks (message_id, block_id, block_type, block_order)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (block_id) DO NOTHING
            "#,
        )
        .bind(block.message_id as i64)
        .bind(&block.block_id)
        .bind(&block.block_type)
        .bind(block.block_order)
        .execute(&self.pool)
        .await
        .context("Failed to insert message block")?;

        Ok(())
    }

    /// メッセージ ID から対応するブロック一覧を取得する。
    pub async fn get_blocks_by_message(&self, message_id: u64) -> Result<Vec<MessageBlock>> {
        sqlx::query_as(
            r#"
            SELECT message_id, block_id, block_type, block_order
            FROM diary_message_blocks
            WHERE message_id = $1
            ORDER BY block_order
            "#,
        )
        .bind(message_id as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch message blocks")
    }

    /// メッセージ ID に対応するブロックをすべて削除する。
    pub async fn delete_blocks_by_message(&self, message_id: u64) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM diary_message_blocks
            WHERE message_id = $1
            "#,
        )
        .bind(message_id as i64)
        .execute(&self.pool)
        .await
        .context("Failed to delete message blocks")?;

        Ok(())
    }

    /// すべての日報エントリを取得する。
    pub async fn get_all_entries(&self) -> Result<Vec<DiaryEntry>> {
        sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            ORDER BY date DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch all diary entries")
    }
}
