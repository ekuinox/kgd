//! 日報エントリとメッセージブロックのドメイン型、日付計算ヘルパー。

use chrono::{DateTime, NaiveDate, NaiveTime, Timelike as _, Utc};
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

/// 日報の「一日」の区切り方。
///
/// 日付の境界を暦日の 0 時ではなく `day_start_hour` に置くことで、
/// 深夜の投稿をその日の日報として扱える。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiaryCalendar {
    /// 日付計算に使用するタイムゾーン
    timezone: Tz,
    /// 一日が始まる時（0-23）
    day_start_hour: u32,
}

impl DiaryCalendar {
    /// 新しい DiaryCalendar を作成する。
    ///
    /// `day_start_hour` は 0-23 の範囲であること。範囲外の値は呼び出し前に弾く。
    pub fn new(timezone: Tz, day_start_hour: u32) -> DiaryCalendar {
        DiaryCalendar {
            timezone,
            day_start_hour,
        }
    }

    /// 日付計算に使用するタイムゾーンを返す。
    pub fn timezone(&self) -> &Tz {
        &self.timezone
    }

    /// 指定された時刻が属する日報日の開始時刻を UTC で返す。
    ///
    /// 戻り値は日報日の暦日 0 時をタイムゾーン基準で表した UTC 時刻であり、
    /// `day_start_hour` を変えても表現は変わらない（永続化済みの日付と互換）。
    pub fn today(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.local_date(now)
            .and_time(NaiveTime::MIN)
            .and_local_timezone(self.timezone)
            .unwrap()
            .to_utc()
    }

    /// 指定された時刻が属する日報日をタイムゾーン基準の暦日として返す。
    pub fn local_date(&self, now: DateTime<Utc>) -> NaiveDate {
        let local = now.with_timezone(&self.timezone);
        if local.hour() < self.day_start_hour {
            // 一日の始まりより前なので前日の日報として扱う
            local
                .date_naive()
                .pred_opt()
                .expect("diary date underflowed the representable range")
        } else {
            local.date_naive()
        }
    }

    /// 日報日を "YYYY-MM-DD" 形式の文字列として返す。
    pub fn format(&self, date: DateTime<Utc>) -> String {
        date.with_timezone(&self.timezone)
            .format("%Y-%m-%d")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// テスト用に Asia/Tokyo の時刻を UTC の DateTime として作る。
    fn jst(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// day_start_hour が 0 のときは暦日がそのまま日報日になり、
    /// 従来の 0 時境界と同じ結果になることを確認する。
    #[test]
    fn today_uses_calendar_day_when_day_start_hour_is_zero() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 0);
        assert_eq!(
            calendar.today(jst(2026, 8, 10, 0, 12)),
            jst(2026, 8, 10, 0, 0)
        );
    }

    /// day_start_hour より前の時刻は前日の日報日として扱われることを確認する。
    #[test]
    fn today_treats_time_before_day_start_hour_as_previous_day() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
        assert_eq!(
            calendar.today(jst(2026, 8, 10, 0, 12)),
            jst(2026, 8, 9, 0, 0)
        );
    }

    /// day_start_hour ちょうどで日報日が切り替わることを確認する。
    #[test]
    fn today_switches_to_new_day_at_day_start_hour() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
        assert_eq!(
            calendar.today(jst(2026, 8, 10, 7, 0)),
            jst(2026, 8, 10, 0, 0)
        );
        assert_eq!(
            calendar.today(jst(2026, 8, 10, 6, 59)),
            jst(2026, 8, 9, 0, 0)
        );
    }

    /// 月をまたぐ場合でも前日の日報日を正しく求められることを確認する。
    #[test]
    fn today_handles_month_boundary() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
        assert_eq!(
            calendar.today(jst(2026, 9, 1, 3, 0)),
            jst(2026, 8, 31, 0, 0)
        );
    }

    /// local_date が暦日ではなく日報日を返すことを確認する。
    #[test]
    fn local_date_returns_diary_day() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
        assert_eq!(
            calendar.local_date(jst(2026, 8, 10, 3, 0)),
            NaiveDate::from_ymd_opt(2026, 8, 9).unwrap()
        );
    }

    /// format が日報日をタイムゾーン基準の "YYYY-MM-DD" で返すことを確認する。
    #[test]
    fn format_returns_date_in_timezone() {
        let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
        let today = calendar.today(jst(2026, 8, 10, 3, 0));
        assert_eq!(calendar.format(today), "2026-08-09");
    }
}
