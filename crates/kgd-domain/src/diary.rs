//! 日報エントリとメッセージブロックのドメイン型、日付計算ヘルパー。

use chrono::{DateTime, NaiveTime, Utc};
use chrono_tz::Tz;

/// 日報スレッドのクローズ & 新規作成ボタンのコンポーネント ID。
///
/// インフラ層（ボタン生成）とプレゼンテーション層（インタラクション判定）で共有する。
pub const DIARY_CLOSE_AND_NEW_BUTTON_ID: &str = "diary_close_and_new";

/// 日報エントリの情報。
#[derive(Debug, Clone)]
pub struct DiaryEntry {
    /// Discord スレッド ID
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

/// メッセージとブロックの対応情報。
#[derive(Debug, Clone)]
pub struct MessageBlock {
    /// Discord メッセージ ID
    pub message_id: u64,
    /// Notion ブロック ID
    pub block_id: String,
    /// ブロックの種類
    pub block_type: String,
    /// ブロックの順序
    pub block_order: i32,
}

/// 書き込み用チャンネルの元メッセージと転記メッセージの対応情報。
#[derive(Debug, Clone)]
pub struct RelayedMessage {
    /// 書き込み用チャンネルの元メッセージ ID
    pub source_message_id: u64,
    /// 転記先の日報スレッド ID
    pub thread_id: u64,
    /// 転記先スレッドに作成された転記メッセージ ID
    pub relayed_message_id: u64,
}

/// 指定された時刻を基準に、そのタイムゾーンでの日付の開始時刻（00:00:00）を UTC で取得する。
///
/// `now` を引数で受けることで、現在時刻に依存しない単体テストが可能になる。
pub fn today_in_timezone(now: DateTime<Utc>, tz: &Tz) -> DateTime<Utc> {
    now.with_timezone(tz)
        .with_time(NaiveTime::MIN)
        .unwrap()
        .to_utc()
}

/// 指定されたタイムゾーンでの日付を "YYYY-MM-DD" 形式の文字列として取得する。
pub fn format_date_in_timezone(date: DateTime<Utc>, tz: &Tz) -> String {
    date.with_timezone(tz).format("%Y-%m-%d").to_string()
}
