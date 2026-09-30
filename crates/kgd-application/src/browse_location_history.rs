//! 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。

use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use chrono_tz::Tz;

use kgd_domain::{
    LocationSummary, TrackSegment, calendar_day_range, count_points, filter_accurate,
    group_by_calendar_day, simplify_segments, split_segments, sum_summaries, summarize,
};

use super::ports::LocationRepository;

/// 期間の位置ログのまとめ方の設定。
#[derive(Debug, Clone, Copy)]
pub struct LocationHistorySettings {
    /// 暦日を区切るタイムゾーン
    pub timezone: Tz,
    /// これを超える水平精度 (メートル) の点を除く
    pub max_accuracy_m: i32,
    /// 地図に返す軌跡の点数の上限
    pub max_track_points: usize,
}

/// 1 日ぶんの集計。
#[derive(Debug, Clone, PartialEq)]
pub struct DailyLocationSummary {
    /// 暦日
    pub date: NaiveDate,
    /// その日の集計
    pub summary: LocationSummary,
}

/// 期間の位置ログのまとめ。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationHistory {
    /// 開始日 (含む)
    pub from: NaiveDate,
    /// 終了日 (含む)
    pub to: NaiveDate,
    /// 暦日を区切ったタイムゾーン
    pub timezone: Tz,
    /// 期間全体の集計 (日ごとの集計の和)
    pub total: LocationSummary,
    /// 日ごとの集計。期間のすべての日を日付順に並べる
    pub days: Vec<DailyLocationSummary>,
    /// 地図に描く軌跡。点数が上限を超えていれば間引いてある
    pub segments: Vec<TrackSegment>,
    /// 間引く前の軌跡の点数 (区間の境目の点は両方の区間で数える)
    pub original_points: usize,
    /// 間引いた後の軌跡の点数 (数え方は `original_points` と同じ)
    pub returned_points: usize,
}

/// 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。
pub struct BrowseLocationHistoryUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
    /// まとめ方の設定
    settings: LocationHistorySettings,
}

impl BrowseLocationHistoryUseCase {
    /// 新しい BrowseLocationHistoryUseCase を作成する。
    pub fn new(repo: Arc<dyn LocationRepository>, settings: LocationHistorySettings) -> Self {
        Self { repo, settings }
    }

    /// `from` から `to` まで (どちらも含む) の位置ログをまとめる。`from <= to` を前提とする。
    ///
    /// 点は 1 回だけ読み、精度の悪い点を日ごとに除いてから集計する。
    /// 期間全体の集計は日ごとの集計の和とし、0 時をまたぐ間隔はどちらの日にも数えない。
    pub async fn browse(&self, from: NaiveDate, to: NaiveDate) -> Result<LocationHistory> {
        let LocationHistorySettings {
            timezone,
            max_accuracy_m,
            max_track_points,
        } = self.settings;
        let (start, end) = calendar_day_range(timezone, from, to);
        let raw = self.repo.locations_between(start, end).await?;

        let mut days = Vec::new();
        let mut kept_points = Vec::new();
        for (date, points) in group_by_calendar_day(raw, timezone, from, to) {
            let (kept, excluded) = filter_accurate(points, max_accuracy_m);
            days.push(DailyLocationSummary {
                date,
                summary: summarize(&kept, excluded),
            });
            kept_points.extend(kept);
        }
        let total = sum_summaries(days.iter().map(|day| &day.summary));

        let segments = split_segments(&kept_points);
        let original_points = count_points(&segments);
        let segments = simplify_segments(segments, max_track_points);
        let returned_points = count_points(&segments);

        Ok(LocationHistory {
            from,
            to,
            timezone,
            total,
            days,
            segments,
            original_points,
            returned_points,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone as _, Utc};
    use mockall::predicate::eq;

    use kgd_domain::{Activity, TrackPoint};

    use crate::ports::MockLocationRepository;

    use super::*;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// 徒歩の点を作る。緯度は分ごとに少しずつずらす。
    fn walk(at: DateTime<Utc>, accuracy_m: i32) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.0 + f64::from(at.timestamp() as i32 % 3600) * 1e-6,
            lon: 139.0,
            accuracy_m: Some(accuracy_m),
            activity: Some(Activity::Walking),
        }
    }

    fn use_case(
        repo: MockLocationRepository,
        max_track_points: usize,
    ) -> BrowseLocationHistoryUseCase {
        BrowseLocationHistoryUseCase::new(
            Arc::new(repo),
            LocationHistorySettings {
                timezone: chrono_tz::Asia::Tokyo,
                max_accuracy_m: 200,
                max_track_points,
            },
        )
    }

    /// 開始日の 0 時から終了日の翌日の 0 時までの範囲で、リポジトリを 1 回だけ読むことを確認する。
    #[tokio::test]
    async fn browse_queries_the_calendar_day_range_once() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .with(eq(jst(1, 0, 0)), eq(jst(3, 0, 0)))
            .times(1)
            .returning(|_, _| Ok(Vec::new()));

        let history = use_case(repo, 100).browse(date(1), date(2)).await.unwrap();

        assert_eq!(history.from, date(1));
        assert_eq!(history.to, date(2));
        assert_eq!(history.timezone, chrono_tz::Asia::Tokyo);
    }

    /// 精度の悪い点は集計と軌跡から外れ、その日の除外数に数えられることを確認する。
    #[tokio::test]
    async fn browse_excludes_inaccurate_points_per_day() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().returning(|_, _| {
            Ok(vec![
                walk(jst(1, 10, 0), 10),
                walk(jst(1, 10, 1), 500),
                walk(jst(1, 10, 2), 10),
            ])
        });

        let history = use_case(repo, 100).browse(date(1), date(1)).await.unwrap();

        assert_eq!(history.days.len(), 1);
        assert_eq!(history.days[0].summary.point_count, 2);
        assert_eq!(history.days[0].summary.excluded_count, 1);
        assert_eq!(history.original_points, 2);
    }

    /// 点の無い日も含めて日付順に並び、合計が日ごとの集計の和になることを確認する。
    ///
    /// グラフの棒の和と合計を一致させるため。
    #[tokio::test]
    async fn browse_lists_every_day_and_totals_the_daily_summaries() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().returning(|_, _| {
            Ok(vec![
                walk(jst(1, 10, 0), 10),
                walk(jst(1, 10, 1), 10),
                walk(jst(3, 9, 0), 10),
                walk(jst(3, 9, 1), 10),
            ])
        });

        let history = use_case(repo, 100).browse(date(1), date(3)).await.unwrap();

        let dates: Vec<NaiveDate> = history.days.iter().map(|day| day.date).collect();
        assert_eq!(dates, vec![date(1), date(2), date(3)]);
        assert_eq!(history.days[1].summary.point_count, 0);
        assert_eq!(history.total.point_count, 4);
        let daily_distance: f64 = history.days.iter().map(|day| day.summary.distance_m).sum();
        assert_eq!(history.total.distance_m, daily_distance);
    }

    /// 点数が上限以下なら間引かず、上限を超えたら上限まで間引くことを確認する。
    #[tokio::test]
    async fn browse_simplifies_only_when_points_exceed_the_limit() {
        let points: Vec<TrackPoint> = (0..50).map(|m| walk(jst(1, 10, m), 10)).collect();
        let mut small = MockLocationRepository::new();
        let returned = points.clone();
        small
            .expect_locations_between()
            .returning(move |_, _| Ok(returned.clone()));
        let mut large = MockLocationRepository::new();
        large
            .expect_locations_between()
            .returning(move |_, _| Ok(points.clone()));

        let untouched = use_case(small, 100).browse(date(1), date(1)).await.unwrap();
        let simplified = use_case(large, 10).browse(date(1), date(1)).await.unwrap();

        assert_eq!(untouched.original_points, 50);
        assert_eq!(untouched.returned_points, 50);
        assert_eq!(simplified.original_points, 50);
        assert_eq!(simplified.returned_points, 10);
        assert_eq!(count_points(&simplified.segments), 10);
    }
}
