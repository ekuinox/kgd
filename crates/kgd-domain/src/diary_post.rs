//! bot が作った内容を日報日のスレッドと Notion ページへ載せるための型。

use chrono::{DateTime, NaiveDate, Utc};

/// 日報に添付する画像。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPostImage {
    /// ファイル名
    pub filename: String,
    /// Content-Type (例: image/png)
    pub content_type: String,
    /// 画像のバイト列
    pub bytes: Vec<u8>,
}

/// 日報日のスレッドと Notion ページへ載せる bot の投稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPost {
    /// 投稿を一意に識別するキー (例: "location-report:2026-09-28")
    pub key: String,
    /// 載せる先の日報日
    pub date: NaiveDate,
    /// 本文 (プレーンテキスト)。Discord の上限 2000 文字以内であること
    pub text: String,
    /// 添付する画像
    pub images: Vec<DiaryPostImage>,
}

/// 日報への投稿の進み具合の記録。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaryPostRecord {
    /// 投稿のキー
    pub key: String,
    /// 載せる先の日報日
    pub diary_date: NaiveDate,
    /// Notion ページへ載せた時刻
    pub notion_posted_at: Option<DateTime<Utc>>,
    /// スレッドへ投稿したメッセージ ID
    pub thread_message_id: Option<u64>,
    /// スレッドへ投稿した時刻
    pub thread_posted_at: Option<DateTime<Utc>>,
    /// これ以上載せないと決めた時刻 (日報が無い、またはスレッドがクローズ済みか見つからない)
    pub skipped_at: Option<DateTime<Utc>>,
}

impl DiaryPostRecord {
    /// Notion ページへ載せ終えているか。
    pub fn notion_done(&self) -> bool {
        self.notion_posted_at.is_some()
    }

    /// スレッドへ載せ終えているか。
    pub fn thread_done(&self) -> bool {
        self.thread_posted_at.is_some()
    }

    /// これ以上何もしなくてよいか (両方済んだ、またはスキップ済み)。
    pub fn is_done(&self) -> bool {
        self.skipped_at.is_some() || (self.notion_done() && self.thread_done())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// 何も済んでいない記録を作る。
    fn record() -> DiaryPostRecord {
        DiaryPostRecord {
            key: "location-report:2026-09-28".to_string(),
            diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            notion_posted_at: None,
            thread_message_id: None,
            thread_posted_at: None,
            skipped_at: None,
        }
    }

    /// Notion とスレッドの両方が済んで初めて完了になることを確認する。
    #[test]
    fn is_done_requires_both_notion_and_thread() {
        let at = Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap();
        let mut notion_only = record();
        notion_only.notion_posted_at = Some(at);
        let mut both = notion_only.clone();
        both.thread_posted_at = Some(at);
        both.thread_message_id = Some(1);

        assert!(!record().is_done());
        assert!(!notion_only.is_done());
        assert!(notion_only.notion_done());
        assert!(!notion_only.thread_done());
        assert!(both.is_done());
    }

    /// スキップ済みの記録は、どちらにも載せていなくても完了とみなすことを確認する。
    ///
    /// 日報が無い日に毎 tick 探し直さないため。
    #[test]
    fn is_done_when_skipped() {
        let mut skipped = record();
        skipped.skipped_at = Some(Utc.with_ymd_and_hms(2026, 9, 28, 23, 0, 0).unwrap());

        assert!(skipped.is_done());
    }
}
