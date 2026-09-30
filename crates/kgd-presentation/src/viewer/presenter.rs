//! ユースケースの結果を API の応答の型に変換する。

use kgd_application::LocationHistory;
use kgd_domain::{Activity, LocationSummary, TrackSegment};

use super::dto::{
    ActivityKind, DailySummary, DistanceByActivity, FeatureCollectionType, FeatureType,
    HistoryRange, HistoryResponse, HistorySummary, LineString, LineStringType, Track, TrackFeature,
    TrackMeta, TrackProperties,
};

/// ユースケースの結果を `GET /viewer/api/history` の応答に変換する。
pub(super) fn present_history(history: &LocationHistory) -> HistoryResponse {
    HistoryResponse {
        range: HistoryRange {
            from: history.from,
            to: history.to,
            timezone: history.timezone.name().to_string(),
        },
        total: present_summary(&history.total),
        days: history
            .days
            .iter()
            .map(|day| DailySummary {
                date: day.date,
                summary: present_summary(&day.summary),
            })
            .collect(),
        track: Track {
            kind: FeatureCollectionType::FeatureCollection,
            features: history.segments.iter().map(present_segment).collect(),
        },
        track_meta: TrackMeta {
            original_points: history.original_points,
            returned_points: history.returned_points,
            simplified: history.returned_points < history.original_points,
        },
    }
}

/// 集計を API の型に変換する。
fn present_summary(summary: &LocationSummary) -> HistorySummary {
    let mut distance = DistanceByActivity {
        walking: 0.0,
        cycling: 0.0,
        automotive: 0.0,
        unknown: 0.0,
    };
    for (activity, meters) in &summary.distance_by_activity {
        match activity {
            Some(Activity::Walking) => distance.walking += meters,
            Some(Activity::Cycling) => distance.cycling += meters,
            Some(Activity::Automotive) => distance.automotive += meters,
            // 静止は距離を積算しないため、集計に現れない
            Some(Activity::Stationary) => {}
            None => distance.unknown += meters,
        }
    }
    HistorySummary {
        distance_m: summary.distance_m,
        distance_by_activity: distance,
        moving_s: summary.moving.num_seconds(),
        stationary_s: summary.stationary.num_seconds(),
        point_count: summary.point_count,
        excluded_count: summary.excluded_count,
        first_at: summary.first_at,
        last_at: summary.last_at,
    }
}

/// 区間を GeoJSON の Feature に変換する。点が 1 つなら同じ座標を 2 つ並べる。
fn present_segment(segment: &TrackSegment) -> TrackFeature {
    let mut coordinates: Vec<[f64; 2]> = segment
        .points
        .iter()
        .map(|point| [point.lon, point.lat])
        .collect();
    if let [only] = coordinates[..] {
        coordinates.push(only);
    }
    TrackFeature {
        kind: FeatureType::Feature,
        properties: TrackProperties {
            activity: activity_kind(segment.activity),
        },
        geometry: LineString {
            kind: LineStringType::LineString,
            coordinates,
        },
    }
}

/// 移動種別を API の型に変換する。
fn activity_kind(activity: Option<Activity>) -> ActivityKind {
    match activity {
        Some(Activity::Walking) => ActivityKind::Walking,
        Some(Activity::Cycling) => ActivityKind::Cycling,
        Some(Activity::Automotive) => ActivityKind::Automotive,
        Some(Activity::Stationary) => ActivityKind::Stationary,
        None => ActivityKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, TimeDelta, TimeZone as _, Utc};

    use kgd_application::DailyLocationSummary;
    use kgd_domain::TrackPoint;

    use super::*;

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap() + TimeDelta::minutes(minute)
    }

    fn point(minute: i64, lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            at: at(minute),
            lat,
            lon,
            accuracy_m: Some(10),
            activity: None,
        }
    }

    fn summary() -> LocationSummary {
        LocationSummary {
            point_count: 3,
            excluded_count: 1,
            distance_m: 1500.0,
            distance_by_activity: vec![(Some(Activity::Automotive), 1000.0), (None, 500.0)],
            moving: TimeDelta::minutes(20),
            stationary: TimeDelta::seconds(90),
            first_at: Some(at(0)),
            last_at: Some(at(30)),
        }
    }

    fn history(segments: Vec<TrackSegment>, original: usize, returned: usize) -> LocationHistory {
        let date = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        LocationHistory {
            from: date,
            to: date,
            timezone: chrono_tz::Asia::Tokyo,
            total: summary(),
            days: vec![DailyLocationSummary {
                date,
                summary: summary(),
            }],
            segments,
            original_points: original,
            returned_points: returned,
        }
    }

    /// 期間と集計が API の型へ写され、移動種別ごとの距離が 4 つの項目に振り分けられることを確認する。
    #[test]
    fn present_history_maps_range_and_summaries() {
        let response = present_history(&history(Vec::new(), 0, 0));

        assert_eq!(response.range.timezone, "Asia/Tokyo");
        assert_eq!(response.range.from.to_string(), "2026-09-01");
        assert_eq!(
            response.total.distance_by_activity,
            DistanceByActivity {
                walking: 0.0,
                cycling: 0.0,
                automotive: 1000.0,
                unknown: 500.0,
            }
        );
        assert_eq!(response.total.moving_s, 1200);
        assert_eq!(response.total.stationary_s, 90);
        assert_eq!(response.total.first_at, Some(at(0)));
        assert_eq!(response.days.len(), 1);
        assert_eq!(response.days[0].date.to_string(), "2026-09-01");
        assert_eq!(response.days[0].summary, response.total);
    }

    /// 区間が `[経度, 緯度]` の LineString になり、移動種別が属性に入ることを確認する。
    #[test]
    fn present_history_turns_segments_into_line_strings() {
        let segments = vec![TrackSegment {
            activity: Some(Activity::Walking),
            points: vec![point(0, 35.0, 139.0), point(1, 35.1, 139.1)],
        }];

        let response = present_history(&history(segments, 2, 2));

        assert_eq!(
            response.track.kind,
            FeatureCollectionType::FeatureCollection
        );
        let feature = &response.track.features[0];
        assert_eq!(feature.properties.activity, ActivityKind::Walking);
        assert_eq!(
            feature.geometry.coordinates,
            vec![[139.0, 35.0], [139.1, 35.1]]
        );
    }

    /// 点が 1 つだけの区間は、同じ座標を 2 つ並べた LineString にすることを確認する。
    ///
    /// GeoJSON の LineString は 2 点以上を要求するため。
    #[test]
    fn present_history_duplicates_single_point_segments() {
        let segments = vec![TrackSegment {
            activity: None,
            points: vec![point(0, 35.0, 139.0)],
        }];

        let response = present_history(&history(segments, 1, 1));

        let feature = &response.track.features[0];
        assert_eq!(feature.properties.activity, ActivityKind::Unknown);
        assert_eq!(
            feature.geometry.coordinates,
            vec![[139.0, 35.0], [139.0, 35.0]]
        );
    }

    /// 間引いた後の点数が間引く前より少ないときだけ `simplified` が true になることを確認する。
    #[test]
    fn present_history_marks_simplified_only_when_points_were_removed() {
        let simplified = present_history(&history(Vec::new(), 30, 20));
        let untouched = present_history(&history(Vec::new(), 20, 20));

        assert_eq!(
            simplified.track_meta,
            TrackMeta {
                original_points: 30,
                returned_points: 20,
                simplified: true,
            }
        );
        assert!(!untouched.track_meta.simplified);
    }
}
