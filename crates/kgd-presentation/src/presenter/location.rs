//! 位置ログのレポートの embed。

use kgd_application::LocationReport;
use kgd_domain::{DiaryCalendar, OSM_ATTRIBUTION, format_location_report};

use super::{EmbedField, EmbedSpec};

/// 位置ログの embed の色。
const LOCATION_COLOR: u32 = 0x2e9e44;

/// 位置ログのレポートの embed を組み立てる。
///
/// 文言は日報に載せる本文と同じ `format_location_report` から作る。
pub fn present_location_report(report: &LocationReport, calendar: &DiaryCalendar) -> EmbedSpec {
    let text = format_location_report(
        report.date,
        report.range,
        &report.summary,
        *calendar.timezone(),
    );
    let field = |name: &str, value: String| EmbedField {
        name: name.to_string(),
        value,
        inline: false,
    };
    EmbedSpec {
        title: text.heading,
        color: LOCATION_COLOR,
        fields: vec![
            field("移動距離", text.distance),
            field("時間", text.durations),
            field("記録", text.points),
            field("時刻", text.times),
        ],
        footer: Some(OSM_ATTRIBUTION.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, TimeDelta, TimeZone as _, Utc};

    use kgd_domain::LocationSummary;

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

    /// embed が見出しをタイトルに、集計を各フィールドに、帰属表示をフッターに持つことを確認する。
    #[test]
    fn present_location_report_builds_embed() {
        let date = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let report = LocationReport {
            date,
            range: (jst(28, 8, 0), jst(29, 8, 0)),
            summary: LocationSummary {
                point_count: 2,
                excluded_count: 0,
                distance_m: 1_000.0,
                distance_by_activity: vec![],
                moving: TimeDelta::minutes(10),
                stationary: TimeDelta::zero(),
                first_at: Some(jst(28, 9, 0)),
                last_at: Some(jst(28, 9, 10)),
            },
            image: Some(vec![1]),
        };

        let embed = present_location_report(&report, &calendar());

        assert_eq!(embed.title, "位置ログ 2026-09-28 (08:00〜翌 08:00)");
        let names: Vec<&str> = embed.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["移動距離", "時間", "記録", "時刻"]);
        assert_eq!(embed.fields[0].value, "1.0 km");
        assert_eq!(
            embed.footer.as_deref(),
            Some("© OpenStreetMap contributors")
        );
    }
}
