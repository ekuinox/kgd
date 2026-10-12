//! コマンドの日付入力の解釈。

use chrono::{DateTime, Datelike as _, NaiveDate, Utc};

use kgd_domain::DiaryCalendar;

/// コマンドの日付入力を解釈できなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateInputError {
    /// YYYY-MM-DD の形式でない
    InvalidFormat,
    /// まだ始まっていない日報日
    NotStarted,
}

impl DateInputError {
    /// 本人へ返す文言。
    pub fn message(&self) -> &'static str {
        match self {
            DateInputError::InvalidFormat => "日付は YYYY-MM-DD の形式で指定してください",
            DateInputError::NotStarted => "まだ始まっていない日報日です",
        }
    }
}

/// コマンドの日付入力から対象の日報日を決める。省略時は現在の日報日。
pub fn resolve_report_date(
    input: Option<&str>,
    calendar: &DiaryCalendar,
    now: DateTime<Utc>,
) -> Result<NaiveDate, DateInputError> {
    let current = calendar.local_date(now);
    let Some(input) = input else {
        return Ok(current);
    };
    let date = parse_diary_date(input)?;
    if date > current {
        return Err(DateInputError::NotStarted);
    }
    Ok(date)
}

/// コマンドの YYYY-MM-DD の日付入力を日報日として解釈する。
///
/// まだ始まっていない日報日かどうかは判断しない。
pub fn parse_diary_date(input: &str) -> Result<NaiveDate, DateInputError> {
    let date = NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| DateInputError::InvalidFormat)?;
    if !(2000..=9999).contains(&date.year()) {
        return Err(DateInputError::InvalidFormat);
    }
    Ok(date)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    fn calendar() -> DiaryCalendar {
        DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8)
    }

    /// 日付を省略すると、現在の日報日になることを確認する。
    ///
    /// day_start_hour より前は前日の日報日が続いている。
    #[test]
    fn resolve_report_date_defaults_to_current_diary_day() {
        assert_eq!(
            resolve_report_date(None, &calendar(), jst(29, 7, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 28).unwrap())
        );
        assert_eq!(
            resolve_report_date(None, &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 29).unwrap())
        );
    }

    /// YYYY-MM-DD で書いた過去の日付と、今の日報日をそのまま受け付けることを確認する。
    #[test]
    fn resolve_report_date_accepts_past_and_current_dates() {
        assert_eq!(
            resolve_report_date(Some("2026-09-01"), &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 1).unwrap())
        );
        assert_eq!(
            resolve_report_date(Some(" 2026-09-29 "), &calendar(), jst(29, 9, 0)),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 29).unwrap())
        );
    }

    /// 書式の違う日付を弾くことを確認する。
    #[test]
    fn resolve_report_date_rejects_invalid_format() {
        assert_eq!(
            resolve_report_date(Some("2026/09/28"), &calendar(), jst(29, 9, 0)),
            Err(DateInputError::InvalidFormat)
        );
        assert_eq!(
            resolve_report_date(Some("yesterday"), &calendar(), jst(29, 9, 0)),
            Err(DateInputError::InvalidFormat)
        );
    }

    /// 極端な年を弾くことを確認する。
    ///
    /// 範囲外の年をそのまま受け付けると、DiaryCalendar::day_range の
    /// succ_opt().expect(...) が chrono の表現できる範囲の端でパニックしうるため。
    #[test]
    fn resolve_report_date_rejects_out_of_range_year() {
        assert_eq!(
            resolve_report_date(Some("1999-12-31"), &calendar(), jst(29, 9, 0)),
            Err(DateInputError::InvalidFormat)
        );
        assert_eq!(
            resolve_report_date(Some("+10000-01-01"), &calendar(), jst(29, 9, 0)),
            Err(DateInputError::InvalidFormat)
        );
    }

    /// まだ始まっていない日報日を弾くことを確認する。
    #[test]
    fn resolve_report_date_rejects_future_date() {
        assert_eq!(
            resolve_report_date(Some("2026-09-29"), &calendar(), jst(29, 7, 0)),
            Err(DateInputError::NotStarted)
        );
    }

    /// YYYY-MM-DD の日付を、まだ始まっていない日報日でも受け付けることを確認する。
    ///
    /// 早出しで次の日報日が既に始まっていることがあるため、未来かどうかは判断しない。
    #[test]
    fn parse_diary_date_accepts_future_date() {
        assert_eq!(
            parse_diary_date(" 2026-09-30 "),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 30).unwrap())
        );
    }
}
