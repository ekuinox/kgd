//! スレッドと Notion ページの紐付け情報を永続化するストア。

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use kgd_application::ports::DiaryRepository;
use kgd_domain::{DiaryEntry, MessageBlock, RelayedMessage};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};

/// diary_entries テーブルの行。
#[derive(Debug, Clone, FromRow)]
struct DiaryEntryRow {
    /// Discord スレッド ID
    #[sqlx(try_from = "i64")]
    thread_id: u64,
    /// Notion ページ ID
    page_id: String,
    /// Notion ページ URL
    page_url: String,
    /// 日付
    date: DateTime<Utc>,
    /// 作成日時
    created_at: DateTime<Utc>,
}

impl From<DiaryEntryRow> for DiaryEntry {
    fn from(row: DiaryEntryRow) -> Self {
        Self {
            thread_id: row.thread_id,
            page_id: row.page_id,
            page_url: row.page_url,
            date: row.date,
            created_at: row.created_at,
        }
    }
}

/// diary_message_blocks テーブルの行。
#[derive(Debug, Clone, FromRow)]
struct MessageBlockRow {
    /// Discord メッセージ ID
    #[sqlx(try_from = "i64")]
    message_id: u64,
    /// Notion ブロック ID
    block_id: String,
    /// ブロックの種類
    block_type: String,
    /// ブロックの順序
    block_order: i32,
}

impl From<MessageBlockRow> for MessageBlock {
    fn from(row: MessageBlockRow) -> Self {
        Self {
            message_id: row.message_id,
            block_id: row.block_id,
            block_type: row.block_type,
            block_order: row.block_order,
        }
    }
}

/// diary_relayed_messages テーブルの行。
#[derive(Debug, Clone, FromRow)]
struct RelayedMessageRow {
    /// 書き込み用チャンネルの元メッセージ ID
    #[sqlx(try_from = "i64")]
    source_message_id: u64,
    /// 転記先の日報スレッド ID
    #[sqlx(try_from = "i64")]
    thread_id: u64,
    /// 転記先スレッドに作成された転記メッセージ ID
    #[sqlx(try_from = "i64")]
    relayed_message_id: u64,
}

impl From<RelayedMessageRow> for RelayedMessage {
    fn from(row: RelayedMessageRow) -> Self {
        Self {
            source_message_id: row.source_message_id,
            thread_id: row.thread_id,
            relayed_message_id: row.relayed_message_id,
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
}

#[async_trait::async_trait]
impl DiaryRepository for DiaryStore {
    async fn insert(&self, entry: &DiaryEntry) -> Result<()> {
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

    async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>> {
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

        Ok(row.map(Into::into))
    }

    async fn get_by_date(&self, date: DateTime<Utc>) -> Result<Option<DiaryEntry>> {
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

        Ok(row.map(Into::into))
    }

    async fn insert_message_block(&self, block: &MessageBlock) -> Result<()> {
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

    async fn get_blocks_by_message(&self, message_id: u64) -> Result<Vec<MessageBlock>> {
        let rows: Vec<MessageBlockRow> = sqlx::query_as(
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
        .context("Failed to fetch message blocks")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn delete_blocks_by_message(&self, message_id: u64) -> Result<()> {
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

    async fn has_blocks_by_message(&self, message_id: u64) -> Result<bool> {
        sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM diary_message_blocks
                WHERE message_id = $1
            )
            "#,
        )
        .bind(message_id as i64)
        .fetch_one(&self.pool)
        .await
        .context("Failed to check message blocks")
    }

    async fn get_entries_in_date_range(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<DiaryEntry>> {
        // 起動時同期で日単位の対象スレッドをまとめて引くため、両端を含む範囲で取得する。
        let rows: Vec<DiaryEntryRow> = sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            WHERE date >= $1 AND date <= $2
            ORDER BY date ASC
            "#,
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch diary entries in date range")?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn get_latest_entry(&self) -> Result<Option<DiaryEntry>> {
        let row: Option<DiaryEntryRow> = sqlx::query_as(
            r#"
            SELECT thread_id, page_id, page_url, date, created_at
            FROM diary_entries
            ORDER BY date DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch latest diary entry")?;

        Ok(row.map(Into::into))
    }

    async fn upsert_relayed_message(&self, relayed: &RelayedMessage) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO diary_relayed_messages (source_message_id, thread_id, relayed_message_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (source_message_id) DO UPDATE SET
                thread_id = EXCLUDED.thread_id,
                relayed_message_id = EXCLUDED.relayed_message_id
            "#,
        )
        .bind(relayed.source_message_id as i64)
        .bind(relayed.thread_id as i64)
        .bind(relayed.relayed_message_id as i64)
        .execute(&self.pool)
        .await
        .context("Failed to upsert relayed message")?;

        Ok(())
    }

    async fn get_relayed_message(&self, source_message_id: u64) -> Result<Option<RelayedMessage>> {
        let row: Option<RelayedMessageRow> = sqlx::query_as(
            r#"
            SELECT source_message_id, thread_id, relayed_message_id
            FROM diary_relayed_messages
            WHERE source_message_id = $1
            "#,
        )
        .bind(source_message_id as i64)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch relayed message")?;

        Ok(row.map(Into::into))
    }

    async fn delete_relayed_message(&self, source_message_id: u64) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM diary_relayed_messages
            WHERE source_message_id = $1
            "#,
        )
        .bind(source_message_id as i64)
        .execute(&self.pool)
        .await
        .context("Failed to delete relayed message")?;

        Ok(())
    }
}
