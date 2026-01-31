//! Discord と Notion を連携した日報機能を提供する。
//!
//! フォーラムスレッドと Notion ページを紐付け、
//! メッセージの同期とライフサイクル管理を行う。

mod notion;
mod store;
mod sync;
mod url_parser;

pub use notion::NotionClient;
pub use store::{DiaryEntry, DiaryStore, MessageBlock};
pub use sync::MessageSyncer;
pub use url_parser::compile_url_rules;

use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;

/// 指定されたタイムゾーンでの現在の日付の開始時刻（00:00:00）を UTC で取得する。
pub fn today_in_timezone(tz: &Tz) -> DateTime<Utc> {
    Utc::now()
        .with_timezone(tz)
        .with_time(NaiveTime::MIN)
        .unwrap()
        .to_utc()
}

/// 指定されたタイムゾーンでの日付を "YYYY-MM-DD" 形式の文字列として取得する。
pub fn format_date_in_timezone(date: DateTime<Utc>, tz: &Tz) -> String {
    date.with_timezone(tz).format("%Y-%m-%d").to_string()
}
