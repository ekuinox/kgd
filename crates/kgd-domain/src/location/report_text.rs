//! 位置ログのレポートの文言。
//!
//! 日報に載せる本文 (application の定時ジョブ) とスラッシュコマンドの embed (presentation) の
//! 両方が使うため、両者が依存できる domain に置く。

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use chrono_tz::Tz;

use super::{summary::LocationSummary, track::Activity};

/// OpenStreetMap のタイルを使うときに必須の帰属表示。
pub const OSM_ATTRIBUTION: &str = "© OpenStreetMap contributors";

/// 位置ログのレポートの文言。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationReportText {
    /// 見出し (日付と範囲)
    pub heading: String,
    /// 移動距離と内訳
    pub distance: String,
    /// 移動と静止の時間
    pub durations: String,
    /// 記録点数と除外数
    pub points: String,
    /// 最初と最後の記録時刻
    pub times: String,
}

impl LocationReportText {
    /// 日報に載せるプレーンテキストの本文を返す。末尾に帰属表示を付ける。
    pub fn to_plain_text(&self) -> String {
        format!(
            "{}\n移動距離 {}\n{}\n記録 {}\n{}\n{}",
            self.heading, self.distance, self.durations, self.points, self.times, OSM_ATTRIBUTION
        )
    }
}

/// 集計値からレポートの文言を組み立てる。
///
/// 時刻は `timezone` で表示する。範囲の終わりが始まりの翌日なら「翌」を付ける。
pub fn format_location_report(
    date: NaiveDate,
    range: (DateTime<Utc>, DateTime<Utc>),
    summary: &LocationSummary,
    timezone: Tz,
) -> LocationReportText {
    let start = range.0.with_timezone(&timezone);
    let end = range.1.with_timezone(&timezone);
    let end_label = if end.date_naive() > start.date_naive() {
        format!("翌 {}", end.format("%H:%M"))
    } else {
        end.format("%H:%M").to_string()
    };

    let distance = if summary.distance_by_activity.is_empty() {
        format_km(summary.distance_m)
    } else {
        let breakdown: Vec<String> = summary
            .distance_by_activity
            .iter()
            .map(|(activity, meters)| {
                format!("{} {}", activity_label(*activity), format_km(*meters))
            })
            .collect();
        format!(
            "{} ({})",
            format_km(summary.distance_m),
            breakdown.join(" / ")
        )
    };

    let points = if summary.excluded_count > 0 {
        format!(
            "{} 点 (精度不足で {} 点を除外)",
            group_thousands(summary.point_count),
            group_thousands(summary.excluded_count)
        )
    } else {
        format!("{} 点", group_thousands(summary.point_count))
    };

    let times = match (summary.first_at, summary.last_at) {
        (Some(first), Some(last)) => format!(
            "最初 {} / 最後 {}",
            first.with_timezone(&timezone).format("%H:%M"),
            last.with_timezone(&timezone).format("%H:%M")
        ),
        _ => "-".to_string(),
    };

    LocationReportText {
        heading: format!(
            "位置ログ {} ({}〜{})",
            date.format("%Y-%m-%d"),
            start.format("%H:%M"),
            end_label
        ),
        distance,
        durations: format!(
            "移動 {} / 静止 {}",
            format_duration(summary.moving),
            format_duration(summary.stationary)
        ),
        points,
        times,
    }
}

/// 記録が無い日の本文を返す。
pub fn format_empty_location_report(date: NaiveDate) -> String {
    format!("位置ログ {} 記録なし", date.format("%Y-%m-%d"))
}

/// メートルを小数 1 桁のキロメートル表記にする。
fn format_km(meters: f64) -> String {
    format!("{:.1} km", meters / 1000.0)
}

/// 時間を「N 時間 M 分」または「M 分」にする。
fn format_duration(duration: TimeDelta) -> String {
    let minutes = duration.num_minutes();
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if hours > 0 {
        format!("{hours} 時間 {minutes} 分")
    } else {
        format!("{minutes} 分")
    }
}

/// 移動種別の表示名。
fn activity_label(activity: Option<Activity>) -> &'static str {
    match activity {
        Some(Activity::Walking) => "徒歩",
        Some(Activity::Cycling) => "自転車",
        Some(Activity::Automotive) => "車",
        Some(Activity::Stationary) => "静止",
        None => "不明",
    }
}

/// 3 桁ごとにカンマで区切る。
fn group_thousands(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
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

    /// 設計書の例と同じ集計から、同じ本文が組み立てられることを確認する。
    #[test]
    fn format_location_report_builds_the_full_text() {
        let summary = LocationSummary {
            point_count: 1234,
            excluded_count: 2,
            distance_m: 12_300.0,
            distance_by_activity: vec![
                (Some(Activity::Walking), 5_100.0),
                (Some(Activity::Automotive), 7_200.0),
            ],
            moving: TimeDelta::minutes(80),
            stationary: TimeDelta::minutes(21 * 60 + 5),
            first_at: Some(jst(28, 8, 3)),
            last_at: Some(jst(29, 7, 58)),
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        let text = format_location_report(
            date,
            (jst(28, 8, 0), jst(29, 8, 0)),
            &summary,
            chrono_tz::Asia::Tokyo,
        );

        assert_eq!(
            text.to_plain_text(),
            "位置ログ 2026-09-28 (08:00〜翌 08:00)\n\
             移動距離 12.3 km (徒歩 5.1 km / 車 7.2 km)\n\
             移動 1 時間 20 分 / 静止 21 時間 5 分\n\
             記録 1,234 点 (精度不足で 2 点を除外)\n\
             最初 08:03 / 最後 07:58\n\
             © OpenStreetMap contributors"
        );
    }

    /// 進行中の日報日 (同じ暦日で終わる範囲) では「翌」を付けず、
    /// 除外が無ければ除外の注記を省き、内訳が無ければ合計だけを出すことを確認する。
    #[test]
    fn format_location_report_handles_partial_day_without_breakdown() {
        let summary = LocationSummary {
            point_count: 3,
            excluded_count: 0,
            distance_m: 0.0,
            distance_by_activity: vec![],
            moving: TimeDelta::zero(),
            stationary: TimeDelta::minutes(20),
            first_at: Some(jst(29, 8, 10)),
            last_at: Some(jst(29, 8, 30)),
        };
        let date = NaiveDate::from_ymd_opt(2026, 9, 29).unwrap();

        let text = format_location_report(
            date,
            (jst(29, 8, 0), jst(29, 14, 32)),
            &summary,
            chrono_tz::Asia::Tokyo,
        );

        assert_eq!(text.heading, "位置ログ 2026-09-29 (08:00〜14:32)");
        assert_eq!(text.distance, "0.0 km");
        assert_eq!(text.durations, "移動 0 分 / 静止 20 分");
        assert_eq!(text.points, "3 点");
        assert_eq!(text.times, "最初 08:10 / 最後 08:30");
    }

    /// 記録が無い日の本文が 1 行になることを確認する。
    #[test]
    fn format_empty_location_report_is_a_single_line() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();

        assert_eq!(
            format_empty_location_report(date),
            "位置ログ 2026-09-28 記録なし"
        );
    }
}
