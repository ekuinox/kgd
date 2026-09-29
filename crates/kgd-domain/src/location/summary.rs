//! 軌跡の距離と時間の集計。

use chrono::{DateTime, TimeDelta, Utc};

use super::track::{Activity, TrackPoint};

/// ハバサイン距離に使う地球の半径 (メートル)。
const EARTH_RADIUS_M: f64 = 6_371_000.0;

/// 時間と距離を積算する間隔の上限 (秒)。これを超える間隔は途切れとみなす。
const MAX_GAP_SECONDS: i64 = 15 * 60;

/// 距離の内訳を並べる順序。
const BREAKDOWN_ORDER: [Option<Activity>; 4] = [
    Some(Activity::Walking),
    Some(Activity::Cycling),
    Some(Activity::Automotive),
    None,
];

/// 1 日ぶんの軌跡の集計値。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationSummary {
    /// 集計に使った点の数
    pub point_count: usize,
    /// 精度不足で除外した点の数
    pub excluded_count: usize,
    /// 移動距離の合計 (メートル)
    pub distance_m: f64,
    /// 移動種別ごとの距離 (メートル)。距離が 0 の種別は含まない
    pub distance_by_activity: Vec<(Option<Activity>, f64)>,
    /// 移動していた時間
    pub moving: TimeDelta,
    /// 静止していた時間
    pub stationary: TimeDelta,
    /// 最初の記録時刻
    pub first_at: Option<DateTime<Utc>>,
    /// 最後の記録時刻
    pub last_at: Option<DateTime<Utc>>,
}

/// 2 点間の大円距離をメートルで返す。引数は `(緯度, 経度)`。
pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((lat2 - lat1) / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// 時刻順に並んだ点列を集計する。
///
/// 隣り合う 2 点の間隔を前の点の移動種別に割り当てる。静止は時間だけを、
/// それ以外 (欠損を含む) は時間と距離を積算する。15 分を超える間隔は数えない。
pub fn summarize(points: &[TrackPoint], excluded_count: usize) -> LocationSummary {
    let mut moving = TimeDelta::zero();
    let mut stationary = TimeDelta::zero();
    let mut distances: Vec<(Option<Activity>, f64)> = BREAKDOWN_ORDER
        .iter()
        .map(|activity| (*activity, 0.0))
        .collect();

    for pair in points.windows(2) {
        let (from, to) = (&pair[0], &pair[1]);
        let gap = to.at - from.at;
        if gap < TimeDelta::zero() || gap.num_seconds() > MAX_GAP_SECONDS {
            continue;
        }
        if from.activity == Some(Activity::Stationary) {
            stationary += gap;
            continue;
        }
        moving += gap;
        let step = haversine_m((from.lat, from.lon), (to.lat, to.lon));
        if let Some(entry) = distances.iter_mut().find(|(a, _)| *a == from.activity) {
            entry.1 += step;
        }
    }

    let distance_m: f64 = distances.iter().map(|(_, d)| d).sum();
    distances.retain(|(_, d)| *d > 0.0);

    LocationSummary {
        point_count: points.len(),
        excluded_count,
        distance_m,
        distance_by_activity: distances,
        moving,
        stationary,
        first_at: points.first().map(|p| p.at),
        last_at: points.last().map(|p| p.at),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use crate::location::track::tests::point;

    use super::*;

    /// 経度 0 の子午線上で緯度 1 度ぶん離れた 2 点の距離が、地球半径から求めた値になることを確認する。
    #[test]
    fn haversine_m_matches_one_degree_of_latitude() {
        let distance = haversine_m((0.0, 0.0), (1.0, 0.0));

        let expected = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;
        assert!((distance - expected).abs() < 1e-6, "distance = {distance}");
    }

    /// 点が無いとき、すべて 0 で時刻も無い集計になることを確認する。
    #[test]
    fn summarize_returns_zero_for_empty_points() {
        let summary = summarize(&[], 3);

        assert_eq!(summary.point_count, 0);
        assert_eq!(summary.excluded_count, 3);
        assert_eq!(summary.distance_m, 0.0);
        assert!(summary.distance_by_activity.is_empty());
        assert_eq!(summary.moving, TimeDelta::zero());
        assert_eq!(summary.stationary, TimeDelta::zero());
        assert_eq!(summary.first_at, None);
        assert_eq!(summary.last_at, None);
    }

    /// 間隔は前の点の種別に割り当てられ、静止は時間だけ、移動は時間と距離が積算されることを確認する。
    ///
    /// 静止中の GPS の揺れを移動距離に数えないため。
    #[test]
    fn summarize_assigns_intervals_to_the_earlier_point_activity() {
        let points = vec![
            point(0, 0.0, 0.0, Some(Activity::Stationary)),
            point(10, 0.001, 0.0, Some(Activity::Walking)),
            point(20, 0.002, 0.0, Some(Activity::Walking)),
        ];

        let summary = summarize(&points, 0);

        let step = haversine_m((0.001, 0.0), (0.002, 0.0));
        assert_eq!(summary.stationary, TimeDelta::minutes(10));
        assert_eq!(summary.moving, TimeDelta::minutes(10));
        assert!((summary.distance_m - step).abs() < 1e-9);
        assert_eq!(summary.distance_by_activity.len(), 1);
        assert_eq!(summary.distance_by_activity[0].0, Some(Activity::Walking));
        assert_eq!(summary.point_count, 3);
        assert_eq!(summary.first_at, Some(points[0].at));
        assert_eq!(summary.last_at, Some(points[2].at));
    }

    /// 15 分を超える間隔は時間にも距離にも数えないことを確認する。
    ///
    /// 圏外などで途切れていた時間を移動や静止として計上しないため。
    #[test]
    fn summarize_skips_intervals_longer_than_fifteen_minutes() {
        let points = vec![
            point(0, 0.0, 0.0, Some(Activity::Automotive)),
            point(15, 0.01, 0.0, Some(Activity::Automotive)),
            point(31, 0.5, 0.0, Some(Activity::Automotive)),
        ];

        let summary = summarize(&points, 0);

        assert_eq!(summary.moving, TimeDelta::minutes(15));
        let first_leg = haversine_m((0.0, 0.0), (0.01, 0.0));
        assert!((summary.distance_m - first_leg).abs() < 1e-9);
    }

    /// 移動種別ごとの距離が、徒歩、自転車、車、欠損の順で並ぶことを確認する。
    #[test]
    fn summarize_orders_distance_breakdown_by_activity() {
        let points = vec![
            point(0, 0.0, 0.0, None),
            point(1, 0.001, 0.0, Some(Activity::Automotive)),
            point(2, 0.002, 0.0, Some(Activity::Walking)),
            point(3, 0.003, 0.0, Some(Activity::Walking)),
        ];

        let summary = summarize(&points, 0);

        let order: Vec<Option<Activity>> = summary
            .distance_by_activity
            .iter()
            .map(|(a, _)| *a)
            .collect();
        assert_eq!(
            order,
            vec![Some(Activity::Walking), Some(Activity::Automotive), None]
        );
    }
}
