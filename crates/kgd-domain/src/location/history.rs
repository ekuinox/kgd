//! 暦日 (タイムゾーンの 0 時区切り) 単位での位置ログの切り出しと集計の足し合わせ。

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use chrono_tz::Tz;

use crate::diary::DiaryCalendar;

use super::{summary::LocationSummary, track::TrackPoint};

/// 開始日から終了日まで (どちらも含む) の範囲を、UTC の半開区間で返す。
///
/// 開始は開始日の 0 時、終了は終了日の翌日の 0 時 (含まない) とする。
pub fn calendar_day_range(
    timezone: Tz,
    from: NaiveDate,
    to: NaiveDate,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let calendar = midnight_calendar(timezone);
    let (start, _) = calendar.day_range(from);
    let (_, end) = calendar.day_range(to);
    (start, end)
}

/// 時刻順の点列を暦日ごとに分ける。
///
/// `from` から `to` までのすべての日を日付順に返し、点の無い日は空の列にする。
/// 範囲の外の点は捨てる。
pub fn group_by_calendar_day(
    points: Vec<TrackPoint>,
    timezone: Tz,
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, Vec<TrackPoint>)> {
    let calendar = midnight_calendar(timezone);
    let mut days: Vec<(NaiveDate, Vec<TrackPoint>)> = from
        .iter_days()
        .take_while(|date| *date <= to)
        .map(|date| (date, Vec::new()))
        .collect();
    for point in points {
        let offset = calendar
            .local_date(point.at)
            .signed_duration_since(from)
            .num_days();
        if let Some((_, bucket)) = usize::try_from(offset)
            .ok()
            .and_then(|index| days.get_mut(index))
        {
            bucket.push(point);
        }
    }
    days
}

/// 日ごとの集計を足し合わせて、期間全体の集計を作る。
///
/// 点数、除外数、距離、移動種別ごとの距離、移動時間、静止時間は和をとり、
/// 最初と最後の記録時刻は最小と最大をとる。
pub fn sum_summaries<'a>(
    summaries: impl IntoIterator<Item = &'a LocationSummary>,
) -> LocationSummary {
    let mut total = LocationSummary {
        point_count: 0,
        excluded_count: 0,
        distance_m: 0.0,
        distance_by_activity: Vec::new(),
        moving: TimeDelta::zero(),
        stationary: TimeDelta::zero(),
        first_at: None,
        last_at: None,
    };
    for summary in summaries {
        total.point_count += summary.point_count;
        total.excluded_count += summary.excluded_count;
        total.distance_m += summary.distance_m;
        total.moving += summary.moving;
        total.stationary += summary.stationary;
        for (activity, distance) in &summary.distance_by_activity {
            match total
                .distance_by_activity
                .iter_mut()
                .find(|(known, _)| known == activity)
            {
                Some(entry) => entry.1 += distance,
                None => total.distance_by_activity.push((*activity, *distance)),
            }
        }
        total.first_at = earlier(total.first_at, summary.first_at);
        total.last_at = later(total.last_at, summary.last_at);
    }
    total
}

/// 0 時で日を区切る暦を返す。
fn midnight_calendar(timezone: Tz) -> DiaryCalendar {
    DiaryCalendar::new(timezone, 0)
}

/// 早いほうの時刻を返す。片方が無ければもう片方を返す。
fn earlier(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// 遅いほうの時刻を返す。片方が無ければもう片方を返す。
fn later(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use crate::location::track::Activity;

    use super::*;

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, month, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    fn point_at(at: DateTime<Utc>) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.68,
            lon: 139.76,
            accuracy_m: Some(10),
            activity: Some(Activity::Walking),
        }
    }

    fn summary(point_count: usize) -> LocationSummary {
        LocationSummary {
            point_count,
            excluded_count: 0,
            distance_m: 0.0,
            distance_by_activity: Vec::new(),
            moving: TimeDelta::zero(),
            stationary: TimeDelta::zero(),
            first_at: None,
            last_at: None,
        }
    }

    /// 範囲が開始日の 0 時から終了日の翌日の 0 時までになることを確認する。
    ///
    /// 終了日もその日を含むため。Asia/Tokyo の 0 時は UTC の前日 15 時にあたる。
    #[test]
    fn calendar_day_range_spans_from_first_midnight_to_midnight_after_last_day() {
        let (start, end) = calendar_day_range(chrono_tz::Asia::Tokyo, date(9, 1), date(9, 2));

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 8, 31, 15, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 9, 2, 15, 0, 0).unwrap());
    }

    /// 0 時の直前と直後の点が別の日に入り、点の無い日も空の列として含まれることを確認する。
    #[test]
    fn group_by_calendar_day_splits_at_local_midnight_and_keeps_empty_days() {
        let before = point_at(jst(9, 1, 23, 59));
        let after = point_at(jst(9, 2, 0, 0));

        let days = group_by_calendar_day(
            vec![before.clone(), after.clone()],
            chrono_tz::Asia::Tokyo,
            date(9, 1),
            date(9, 3),
        );

        assert_eq!(
            days,
            vec![
                (date(9, 1), vec![before]),
                (date(9, 2), vec![after]),
                (date(9, 3), vec![]),
            ]
        );
    }

    /// 範囲の外の点は捨てることを確認する。
    ///
    /// リポジトリは範囲内の点だけを返す約束だが、日付の添字がはみ出してパニックしないようにする。
    #[test]
    fn group_by_calendar_day_drops_points_outside_the_range() {
        let days = group_by_calendar_day(
            vec![point_at(jst(8, 31, 12, 0)), point_at(jst(9, 2, 12, 0))],
            chrono_tz::Asia::Tokyo,
            date(9, 1),
            date(9, 1),
        );

        assert_eq!(days, vec![(date(9, 1), vec![])]);
    }

    /// 各項目の和をとり、最初と最後の記録時刻は最小と最大をとることを確認する。
    #[test]
    fn sum_summaries_adds_counts_and_durations_and_takes_min_max_times() {
        let first = LocationSummary {
            point_count: 3,
            excluded_count: 1,
            distance_m: 100.0,
            distance_by_activity: vec![(Some(Activity::Walking), 100.0)],
            moving: TimeDelta::minutes(10),
            stationary: TimeDelta::minutes(5),
            first_at: Some(jst(9, 1, 8, 0)),
            last_at: Some(jst(9, 1, 20, 0)),
        };
        let second = LocationSummary {
            point_count: 2,
            excluded_count: 2,
            distance_m: 250.0,
            distance_by_activity: vec![
                (Some(Activity::Walking), 50.0),
                (Some(Activity::Automotive), 200.0),
            ],
            moving: TimeDelta::minutes(20),
            stationary: TimeDelta::minutes(1),
            first_at: Some(jst(9, 2, 9, 0)),
            last_at: Some(jst(9, 2, 22, 0)),
        };

        let total = sum_summaries([&first, &summary(0), &second]);

        assert_eq!(total.point_count, 5);
        assert_eq!(total.excluded_count, 3);
        assert_eq!(total.distance_m, 350.0);
        assert_eq!(
            total.distance_by_activity,
            vec![
                (Some(Activity::Walking), 150.0),
                (Some(Activity::Automotive), 200.0),
            ]
        );
        assert_eq!(total.moving, TimeDelta::minutes(30));
        assert_eq!(total.stationary, TimeDelta::minutes(6));
        assert_eq!(total.first_at, Some(jst(9, 1, 8, 0)));
        assert_eq!(total.last_at, Some(jst(9, 2, 22, 0)));
    }

    /// 集計が 1 つも無いとき、すべて 0 で時刻も無い集計になることを確認する。
    #[test]
    fn sum_summaries_returns_zero_for_no_summaries() {
        assert_eq!(sum_summaries([]), summary(0));
    }
}
