//! Discord と Notion を連携した日報機能を提供する。
//!
//! フォーラムスレッドと Notion ページを紐付け、
//! メッセージの同期とライフサイクル管理を行う。

mod notion;
mod store;
mod sync;

pub use notion::NotionClient;
pub use store::{DiaryEntry, DiaryStore};
pub use sync::MessageSyncer;

use chrono::NaiveDate;
use chrono_tz::Asia::Tokyo;

/// 現在の JST 日付を取得する。
pub fn today_jst() -> NaiveDate {
    let now = chrono::Utc::now().with_timezone(&Tokyo);
    now.date_naive()
}
