//! 日報エントリとメッセージブロックの永続化を抽象化するポート。

use anyhow::Result;
use chrono::{DateTime, Utc};

use kgd_domain::{DiaryEntry, MessageBlock, RelayedMessage};

/// 日報エントリとメッセージブロックの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait DiaryRepository: Send + Sync {
    /// エントリを追加する (thread_id が重複する場合は上書き)。
    async fn insert(&self, entry: &DiaryEntry) -> Result<()>;

    /// スレッド ID からエントリを取得する。
    async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>>;

    /// 日付からエントリを取得する。
    async fn get_by_date(&self, date: DateTime<Utc>) -> Result<Option<DiaryEntry>>;

    /// メッセージとブロックの対応を保存する。
    async fn insert_message_block(&self, block: &MessageBlock) -> Result<()>;

    /// メッセージ ID から対応するブロック一覧を取得する。
    async fn get_blocks_by_message(&self, message_id: u64) -> Result<Vec<MessageBlock>>;

    /// メッセージ ID に対応するブロックをすべて削除する。
    async fn delete_blocks_by_message(&self, message_id: u64) -> Result<()>;

    /// メッセージ ID に紐づくブロックが存在するかどうかを返す。
    async fn has_blocks_by_message(&self, message_id: u64) -> Result<bool>;

    /// 指定した日付範囲に含まれるエントリを古い順で取得する (両端を含む)。
    async fn get_entries_in_date_range(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<DiaryEntry>>;

    /// 最新の日報エントリを取得する。
    async fn get_latest_entry(&self) -> Result<Option<DiaryEntry>>;

    /// 元メッセージと転記メッセージの対応を保存する。
    ///
    /// 同じ元メッセージに対しては転記先の情報を上書きする。
    async fn upsert_relayed_message(&self, relayed: &RelayedMessage) -> Result<()>;

    /// 元メッセージ ID から転記メッセージの対応を取得する。
    async fn get_relayed_message(&self, source_message_id: u64) -> Result<Option<RelayedMessage>>;

    /// 元メッセージ ID に対応する転記メッセージの対応を削除する。
    async fn delete_relayed_message(&self, source_message_id: u64) -> Result<()>;
}
