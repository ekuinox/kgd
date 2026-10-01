//! 点数の上限に合わせた軌跡の間引き (Visvalingam-Whyatt 法)。

use std::{cmp::Ordering, collections::BinaryHeap};

use super::track::{TrackPoint, TrackSegment};

/// 区間の点の数の和を返す。区間の境目で共有する点は両方の区間で数える。
pub fn count_points(segments: &[TrackSegment]) -> usize {
    segments.iter().map(|segment| segment.points.len()).sum()
}

/// 点数の合計が `max_points` 以下になるまで、Visvalingam-Whyatt 法で点を間引く。
///
/// すべての区間の内側の点を 1 つの優先度付きキューで扱い、前後の点と作る三角形の
/// 面積が最も小さい点から取り除く。区間の始点と終点は取り除かないため、
/// 始点と終点だけで上限を超えるときはそこで止める。点数が上限以下なら何もしない。
pub fn simplify_segments(segments: Vec<TrackSegment>, max_points: usize) -> Vec<TrackSegment> {
    let mut remaining = count_points(&segments);
    if remaining <= max_points {
        return segments;
    }

    let mut links: Vec<Vec<Link>> = segments
        .iter()
        .map(|segment| {
            let len = segment.points.len();
            (0..len)
                .map(|index| Link {
                    prev: index.checked_sub(1),
                    next: (index + 1 < len).then_some(index + 1),
                    removed: false,
                    version: 0,
                })
                .collect()
        })
        .collect();

    let mut heap = BinaryHeap::new();
    for (segment_index, segment) in segments.iter().enumerate() {
        let points = &segment.points;
        for index in 1..points.len().saturating_sub(1) {
            heap.push(Candidate {
                area: triangle_area(&points[index - 1], &points[index], &points[index + 1]),
                segment: segment_index,
                index,
                version: 0,
            });
        }
    }

    while remaining > max_points {
        let Some(candidate) = heap.pop() else {
            break;
        };
        let segment_links = &mut links[candidate.segment];
        let link = segment_links[candidate.index];
        if link.removed || link.version != candidate.version {
            continue;
        }
        let (Some(prev), Some(next)) = (link.prev, link.next) else {
            continue;
        };
        segment_links[candidate.index].removed = true;
        segment_links[prev].next = Some(next);
        segment_links[next].prev = Some(prev);
        remaining -= 1;

        let points = &segments[candidate.segment].points;
        for neighbor in [prev, next] {
            let neighbor_link = segment_links[neighbor];
            let (Some(before), Some(after)) = (neighbor_link.prev, neighbor_link.next) else {
                continue;
            };
            let version = neighbor_link.version + 1;
            segment_links[neighbor].version = version;
            // 取り除いた点より小さい面積にしない (Visvalingam-Whyatt 法の慣例)。
            // 隣の点が取り除いた点より先に消えて、形の崩れる順序が逆転するのを防ぐ。
            let area = triangle_area(&points[before], &points[neighbor], &points[after])
                .max(candidate.area);
            heap.push(Candidate {
                area,
                segment: candidate.segment,
                index: neighbor,
                version,
            });
        }
    }

    segments
        .into_iter()
        .zip(links)
        .map(|(segment, links)| TrackSegment {
            activity: segment.activity,
            points: segment
                .points
                .into_iter()
                .zip(links)
                .filter(|(_, link)| !link.removed)
                .map(|(point, _)| point)
                .collect(),
        })
        .collect()
}

/// 区間内での点の前後のつながり。
#[derive(Debug, Clone, Copy)]
struct Link {
    /// 残っている直前の点の添字
    prev: Option<usize>,
    /// 残っている直後の点の添字
    next: Option<usize>,
    /// 取り除いたかどうか
    removed: bool,
    /// 面積を計算し直した回数。古い候補を見分けるのに使う
    version: u64,
}

/// 取り除く候補の点。面積が小さいほど先に取り出す。
#[derive(Debug)]
struct Candidate {
    /// 前後の点と作る三角形の面積
    area: f64,
    /// 区間の添字
    segment: usize,
    /// 区間内での点の添字
    index: usize,
    /// 候補を作ったときの `Link::version`
    version: u64,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap は最大のものを先に返すため、面積の比較を逆にする。
        // 面積が同じなら前の区間、前の点を先に返し、結果を決定的にする。
        other
            .area
            .total_cmp(&self.area)
            .then_with(|| other.segment.cmp(&self.segment))
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Candidate {}

/// 3 点が作る三角形の面積を返す。
///
/// 経度に中央の点の緯度の余弦を掛けて、狭い範囲を平面とみなした座標で計算する。
/// 大小の比較にだけ使うため、単位は度の 2 乗のままにする。
fn triangle_area(a: &TrackPoint, b: &TrackPoint, c: &TrackPoint) -> f64 {
    let scale = b.lat.to_radians().cos();
    let (ax, ay) = (a.lon * scale, a.lat);
    let (bx, by) = (b.lon * scale, b.lat);
    let (cx, cy) = (c.lon * scale, c.lat);
    ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs() / 2.0
}

#[cfg(test)]
mod tests {
    use crate::location::track::{Activity, tests::point};

    use super::*;

    /// 緯度方向にジグザグする点列の区間を作る。
    fn zigzag(activity: Option<Activity>, start_minute: i64, count: usize) -> TrackSegment {
        TrackSegment {
            activity,
            points: (0..count)
                .map(|i| {
                    let lat = 35.0 + if i % 2 == 0 { 0.0 } else { 0.001 * (i as f64) };
                    point(
                        start_minute + i as i64,
                        lat,
                        139.0 + 0.001 * i as f64,
                        activity,
                    )
                })
                .collect(),
        }
    }

    /// 点数が上限以下なら何も変えないことを確認する。
    #[test]
    fn simplify_segments_keeps_segments_within_the_limit() {
        let segments = vec![zigzag(Some(Activity::Walking), 0, 5)];

        assert_eq!(simplify_segments(segments.clone(), 5), segments);
    }

    /// 点数の合計が上限ちょうどまで減ることを確認する。
    #[test]
    fn simplify_segments_reduces_points_to_the_limit() {
        let segments = vec![
            zigzag(Some(Activity::Walking), 0, 60),
            zigzag(Some(Activity::Automotive), 100, 40),
        ];

        let simplified = simplify_segments(segments, 20);

        assert_eq!(count_points(&simplified), 20);
    }

    /// 区間の始点と終点が必ず残り、区間の数と移動種別が変わらないことを確認する。
    ///
    /// 移動種別の色の切れ目の位置を変えないため。
    #[test]
    fn simplify_segments_keeps_segment_endpoints_and_activities() {
        let segments = vec![
            zigzag(Some(Activity::Walking), 0, 30),
            zigzag(None, 100, 30),
        ];

        let simplified = simplify_segments(segments.clone(), 8);

        assert_eq!(simplified.len(), 2);
        for (before, after) in segments.iter().zip(&simplified) {
            assert_eq!(after.activity, before.activity);
            assert_eq!(after.points.first(), before.points.first());
            assert_eq!(after.points.last(), before.points.last());
        }
    }

    /// 形への影響が最も小さい点 (直線上の点) から取り除くことを確認する。
    #[test]
    fn simplify_segments_removes_the_least_significant_point_first() {
        let straight = point(1, 35.0, 139.001, None);
        let spike = point(3, 35.01, 139.003, None);
        let segment = TrackSegment {
            activity: None,
            points: vec![
                point(0, 35.0, 139.0, None),
                straight.clone(),
                point(2, 35.0, 139.002, None),
                spike.clone(),
                point(4, 35.0, 139.004, None),
            ],
        };

        let simplified = simplify_segments(vec![segment], 4);

        assert!(!simplified[0].points.contains(&straight));
        assert!(simplified[0].points.contains(&spike));
    }

    /// 始点と終点だけで上限を超えるときは、それ以上は間引かずに返すことを確認する。
    #[test]
    fn simplify_segments_stops_when_only_endpoints_remain() {
        let segments: Vec<TrackSegment> = (0..5)
            .map(|i| zigzag(Some(Activity::Walking), i * 10, 3))
            .collect();

        let simplified = simplify_segments(segments, 4);

        assert_eq!(count_points(&simplified), 10);
        assert!(simplified.iter().all(|segment| segment.points.len() == 2));
    }

    /// 1 年ぶん (約 40 万点) の軌跡も上限まで間引けることを確認する。
    ///
    /// 1 日に 1000 点ほど記録されるため、長い期間を選ぶとこの規模になる。
    /// 区間ごとに間引くと面積を何度も計算し直すことになるが、優先度付きキューで
    /// 全体を一度に扱うため、デバッグビルドのテストでも数秒以内に終わる。
    #[test]
    fn simplify_segments_handles_a_year_of_points() {
        let segments: Vec<TrackSegment> = (0..400)
            .map(|i| {
                let activity = if i % 2 == 0 {
                    Some(Activity::Walking)
                } else {
                    None
                };
                zigzag(activity, i * 2000, 1000)
            })
            .collect();

        let simplified = simplify_segments(segments, 20000);

        assert_eq!(count_points(&simplified), 20000);
    }
}
