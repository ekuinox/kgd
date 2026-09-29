//! 日報への bot の投稿の進み具合を永続化するポート。

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};

use kgd_domain::DiaryPostRecord;

/// 日報への bot の投稿の進み具合を永続化するポート。
///
/// 段ごとに完了を記録し、途中で失敗しても残りの段だけをやり直せるようにする。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait DiaryPostRepository: Send + Sync {
    /// キーに対応する記録を返す。まだ無ければ `None`。
    async fn get(&self, key: &str) -> Result<Option<DiaryPostRecord>>;

    /// Notion ページへ載せ終えたことを記録する。
    async fn mark_notion_posted(&self, key: &str, date: NaiveDate, at: DateTime<Utc>)
    -> Result<()>;

    /// スレッドへ投稿し終えたことを、投稿したメッセージ ID とともに記録する。
    async fn mark_thread_posted(
        &self,
        key: &str,
        date: NaiveDate,
        message_id: u64,
        at: DateTime<Utc>,
    ) -> Result<()>;

    /// 日報が無いためスキップしたことを記録する。
    async fn mark_skipped(&self, key: &str, date: NaiveDate, at: DateTime<Utc>) -> Result<()>;
}
