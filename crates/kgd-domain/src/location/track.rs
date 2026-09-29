//! 軌跡の点と、外れ値の除去・移動種別による区間分割。

use chrono::{DateTime, Utc};

/// 移動種別 (OwnTracks の motionactivities の先頭要素)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activity {
    /// 徒歩 (running を含む)
    Walking,
    /// 自転車
    Cycling,
    /// 車などの乗り物
    Automotive,
    /// 静止
    Stationary,
}

impl Activity {
    /// motionactivities の文字列から移動種別を得る。
    ///
    /// `unknown` と未知の値は欠損 (`None`) として扱う。
    pub fn from_motion(motion: &str) -> Option<Activity> {
        match motion {
            "walking" | "running" => Some(Activity::Walking),
            "cycling" => Some(Activity::Cycling),
            "automotive" => Some(Activity::Automotive),
            "stationary" => Some(Activity::Stationary),
            _ => None,
        }
    }
}

/// 軌跡を構成する 1 点。
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPoint {
    /// 端末が位置を取得した時刻
    pub at: DateTime<Utc>,
    /// 緯度
    pub lat: f64,
    /// 経度
    pub lon: f64,
    /// 水平精度 (メートル)。不明なら None
    pub accuracy_m: Option<i32>,
    /// 移動種別。欠損なら None
    pub activity: Option<Activity>,
}

/// 同じ移動種別が続く軌跡の区間。
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSegment {
    /// 区間の移動種別。欠損なら None
    pub activity: Option<Activity>,
    /// 区間の点 (2 つ目以降の区間は直前の区間の最後の点から始まる)
    pub points: Vec<TrackPoint>,
}

/// 精度がしきい値を超える点を除き、残した点と除外した件数を返す。
///
/// 精度が不明な点は残す。
pub fn filter_accurate(points: Vec<TrackPoint>, max_accuracy_m: i32) -> (Vec<TrackPoint>, usize) {
    let total = points.len();
    let kept: Vec<TrackPoint> = points
        .into_iter()
        .filter(|point| point.accuracy_m.is_none_or(|acc| acc <= max_accuracy_m))
        .collect();
    let excluded = total - kept.len();
    (kept, excluded)
}

/// 点列を移動種別が続く区間に分ける。
///
/// 欠損は補完せず、`None` を 1 つの種別として扱う。
/// 区間の境目で線が途切れないよう、2 つ目以降の区間は直前の区間の最後の点から始める。
pub fn split_segments(points: &[TrackPoint]) -> Vec<TrackSegment> {
    let mut segments: Vec<TrackSegment> = Vec::new();
    for point in points {
        match segments.last_mut() {
            Some(current) if current.activity == point.activity => {
                current.points.push(point.clone());
            }
            Some(current) => {
                let bridge = current
                    .points
                    .last()
                    .cloned()
                    .expect("segments always hold at least one point");
                segments.push(TrackSegment {
                    activity: point.activity,
                    points: vec![bridge, point.clone()],
                });
            }
            None => segments.push(TrackSegment {
                activity: point.activity,
                points: vec![point.clone()],
            }),
        }
    }
    segments
}

#[cfg(test)]
pub(crate) mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::*;

    /// テスト用の点を作る。`minute` は 2026-09-28 00:00 UTC からの分。
    pub(crate) fn point(minute: i64, lat: f64, lon: f64, activity: Option<Activity>) -> TrackPoint {
        TrackPoint {
            at: Utc.with_ymd_and_hms(2026, 9, 28, 0, 0, 0).unwrap()
                + chrono::TimeDelta::minutes(minute),
            lat,
            lon,
            accuracy_m: Some(10),
            activity,
        }
    }

    /// OwnTracks の motionactivities の値が移動種別へ対応づけられることを確認する。
    ///
    /// running は徒歩として扱い、unknown や未知の値は欠損として扱う。
    #[test]
    fn from_motion_maps_known_values_and_treats_others_as_missing() {
        assert_eq!(Activity::from_motion("walking"), Some(Activity::Walking));
        assert_eq!(Activity::from_motion("running"), Some(Activity::Walking));
        assert_eq!(Activity::from_motion("cycling"), Some(Activity::Cycling));
        assert_eq!(
            Activity::from_motion("automotive"),
            Some(Activity::Automotive)
        );
        assert_eq!(
            Activity::from_motion("stationary"),
            Some(Activity::Stationary)
        );
        assert_eq!(Activity::from_motion("unknown"), None);
        assert_eq!(Activity::from_motion("flying"), None);
    }

    /// 精度がしきい値を超える点だけが除外され、その件数が返ることを確認する。
    ///
    /// 実測データに acc = 1414 の点があり、残すと軌跡が大きく飛ぶため。
    #[test]
    fn filter_accurate_drops_points_above_threshold() {
        let mut far = point(1, 35.0, 139.0, None);
        far.accuracy_m = Some(1414);
        let mut edge = point(2, 35.0, 139.0, None);
        edge.accuracy_m = Some(200);
        let mut unknown = point(3, 35.0, 139.0, None);
        unknown.accuracy_m = None;

        let (kept, excluded) = filter_accurate(vec![far, edge.clone(), unknown.clone()], 200);

        assert_eq!(kept, vec![edge, unknown]);
        assert_eq!(excluded, 1);
    }

    /// 空の点列からは区間が生まれないことを確認する。
    #[test]
    fn split_segments_returns_nothing_for_empty_input() {
        assert!(split_segments(&[]).is_empty());
    }

    /// 移動種別が変わるたびに区間が分かれ、新しい区間は直前の区間の最後の点から始まることを確認する。
    ///
    /// 区間の境目で線が途切れないようにするため。
    #[test]
    fn split_segments_starts_each_segment_from_previous_last_point() {
        let walk1 = point(0, 35.0, 139.0, Some(Activity::Walking));
        let walk2 = point(1, 35.001, 139.0, Some(Activity::Walking));
        let car1 = point(2, 35.002, 139.0, Some(Activity::Automotive));
        let car2 = point(3, 35.003, 139.0, Some(Activity::Automotive));

        let segments = split_segments(&[walk1.clone(), walk2.clone(), car1.clone(), car2.clone()]);

        assert_eq!(
            segments,
            vec![
                TrackSegment {
                    activity: Some(Activity::Walking),
                    points: vec![walk1, walk2.clone()],
                },
                TrackSegment {
                    activity: Some(Activity::Automotive),
                    points: vec![walk2, car1, car2],
                },
            ]
        );
    }

    /// 欠損 (None) も 1 つの種別として区間になり、補完されないことを確認する。
    #[test]
    fn split_segments_keeps_missing_activity_as_its_own_segment() {
        let walk = point(0, 35.0, 139.0, Some(Activity::Walking));
        let missing1 = point(1, 35.001, 139.0, None);
        let missing2 = point(2, 35.002, 139.0, None);

        let segments = split_segments(&[walk, missing1, missing2]);

        let activities: Vec<Option<Activity>> = segments.iter().map(|s| s.activity).collect();
        assert_eq!(activities, vec![Some(Activity::Walking), None]);
        assert_eq!(segments[1].points.len(), 3);
    }

    /// すべて欠損の点列が 1 つの区間になることを確認する。
    #[test]
    fn split_segments_groups_all_missing_points_into_one_segment() {
        let points: Vec<TrackPoint> = (0..3).map(|m| point(m, 35.0, 139.0, None)).collect();

        let segments = split_segments(&points);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].activity, None);
        assert_eq!(segments[0].points.len(), 3);
    }
}
