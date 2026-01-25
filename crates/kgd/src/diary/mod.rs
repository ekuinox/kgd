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

use chrono::{DateTime, Utc};
use chrono_tz::Asia::Tokyo;

/// 現在の JST 日付の開始時刻（00:00:00 JST）を UTC で取得する。
pub fn today_jst() -> DateTime<Utc> {
    let now = chrono::Utc::now().with_timezone(&Tokyo);
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
        .and_local_timezone(Tokyo)
        .unwrap()
        .to_utc()
}
