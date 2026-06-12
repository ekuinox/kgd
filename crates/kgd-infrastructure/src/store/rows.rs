//! データベーステーブルの行型とドメイン型への変換。

use chrono::{DateTime, Utc};
use sqlx::FromRow;

use kgd_domain::{DiaryEntry, MessageBlock, RelayedMessage};

/// diary_entries テーブルの行。
#[derive(Debug, Clone, FromRow)]
pub(super) struct DiaryEntryRow {
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
pub(super) struct MessageBlockRow {
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
pub(super) struct RelayedMessageRow {
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
