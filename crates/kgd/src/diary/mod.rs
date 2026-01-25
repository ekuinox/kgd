//! Discord と Notion を連携した日報機能を提供する。
//!
//! フォーラムスレッドと Notion ページを紐付け、
//! メッセージの同期とライフサイクル管理を行う。

mod notion;
mod store;
mod sync;

pub use notion::NotionClient;
pub use store::{DiaryEntry, DiaryStore, MessageBlock};
pub use sync::MessageSyncer;

use chrono::{DateTime, Local, NaiveTime, Utc};

/// 現在のローカル日付の開始時刻（00:00:00）を UTC で取得する。
pub fn today_local() -> DateTime<Utc> {
    Local::now().with_time(NaiveTime::MIN).unwrap().to_utc()
}
