//! 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。

use std::{ops::RangeInclusive, sync::Arc};

use anyhow::Context as _;
use chrono::{Datelike as _, NaiveDate};
use chrono_tz::Tz;
use tokio::sync::Semaphore;

use kgd_domain::{
    LocationSummary, TrackPoint, TrackSegment, calendar_day_range, count_points, filter_accurate,
    group_by_calendar_day, simplify_segments, split_segments, sum_summaries, summarize,
};

use super::ports::LocationRepository;

/// 一度に選べる期間の上限 (日数)。
///
/// 期間の長さは原則として制限しないが、年の桁を誤った URL で数十万日ぶんの
/// 日ごとの集計を作らないよう、10 年を超える範囲だけは拒否する。
const MAX_HISTORY_RANGE_DAYS: i64 = 3660;

/// 受け付ける日付の年の範囲。
///
/// 応答の日付を `YYYY-MM-DD` の 4 桁の年で表せる範囲に絞る。
const YEAR_RANGE: RangeInclusive<i32> = 1..=9999;

/// 同時にまとめる期間の数の上限。
///
/// 長い期間は点を数百万件読み、間引きのために同じくらいの作業領域を使う。
/// 同時に何件も受けると Raspberry Pi のメモリが足りなくなるため、1 件ずつ処理する。
const MAX_CONCURRENT_BROWSES: usize = 1;

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

/// 受け付けられない期間。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HistoryRangeError {
    /// 開始日が終了日より後にある
    #[error("from must not be after to")]
    Reversed,
    /// 期間が上限より長い
    #[error("the range must be at most {MAX_HISTORY_RANGE_DAYS} days")]
    TooLong,
    /// 年が受け付ける範囲の外にある
    #[error("the year must be in {}-{}", YEAR_RANGE.start(), YEAR_RANGE.end())]
    OutOfRange,
}

/// 期間の位置ログをまとめられなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum BrowseLocationHistoryError {
    /// 期間が受け付けられない (利用者の入力の誤り)
    #[error(transparent)]
    InvalidRange(#[from] HistoryRangeError),
    /// 位置ログの読み出しやまとめる処理に失敗した
    #[error(transparent)]
    Failed(#[from] anyhow::Error),
}

/// 暦日で選んだ期間の位置ログを、集計と地図用の軌跡にまとめるユースケース。
pub struct BrowseLocationHistoryUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
    /// まとめ方の設定
    settings: LocationHistorySettings,
    /// 同時にまとめる期間の数を絞るセマフォ
    permits: Arc<Semaphore>,
}

impl BrowseLocationHistoryUseCase {
    /// 新しい BrowseLocationHistoryUseCase を作成する。
    pub fn new(repo: Arc<dyn LocationRepository>, settings: LocationHistorySettings) -> Self {
        Self {
            repo,
            settings,
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_BROWSES)),
        }
    }

    /// 暦日を区切るタイムゾーンを返す。
    ///
    /// 画面が「今日」をサーバーと同じ暦で決めるために使う。
    pub fn timezone(&self) -> Tz {
        self.settings.timezone
    }

    /// `from` から `to` まで (どちらも含む) の位置ログをまとめる。
    ///
    /// 期間が逆向き、長すぎる、年が範囲の外にあるときは、リポジトリを読まずに
    /// [`BrowseLocationHistoryError::InvalidRange`] を返す。
    /// 点は 1 回だけ読み、精度の悪い点を日ごとに除いてから集計する。
    /// 期間全体の集計は日ごとの集計の和とし、0 時をまたぐ間隔はどちらの日にも数えない。
    /// 集計と間引きは CPU を長く使うため、非同期のワーカーを塞がないよう別のスレッドで行う。
    pub async fn browse(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<LocationHistory, BrowseLocationHistoryError> {
        validate_range(from, to)?;
        let (start, end) = calendar_day_range(self.settings.timezone, from, to)
            .ok_or(HistoryRangeError::OutOfRange)?;
        // 取り消されたリクエストのぶんも、まとめ終わるまで枠を返さないよう、
        // 枠は別のスレッドの処理へ渡す
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .context("the browse semaphore was closed")?;
        let raw = self.repo.locations_between(start, end).await?;
        let settings = self.settings;
        let history = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            build_history(raw, settings, from, to)
        })
        .await
        .context("failed to join the location history task")?;
        Ok(history)
    }
}

/// 期間が受け付けられるかを確かめる。
fn validate_range(from: NaiveDate, to: NaiveDate) -> Result<(), HistoryRangeError> {
    if !YEAR_RANGE.contains(&from.year()) || !YEAR_RANGE.contains(&to.year()) {
        return Err(HistoryRangeError::OutOfRange);
    }
    if from > to {
        return Err(HistoryRangeError::Reversed);
    }
    if (to - from).num_days() >= MAX_HISTORY_RANGE_DAYS {
        return Err(HistoryRangeError::TooLong);
    }
    Ok(())
}

/// 読み出した点を、日ごとの集計と地図用の軌跡にまとめる。
///
/// 点を複製しないよう、日ごとに残した点は区間へ移しながら手放す。
fn build_history(
    raw: Vec<TrackPoint>,
    settings: LocationHistorySettings,
    from: NaiveDate,
    to: NaiveDate,
) -> LocationHistory {
    let LocationHistorySettings {
        timezone,
        max_accuracy_m,
        max_track_points,
    } = settings;

    let mut days = Vec::new();
    let mut kept_by_day = Vec::new();
    for (date, points) in group_by_calendar_day(raw, timezone, from, to) {
        let (kept, excluded) = filter_accurate(points, max_accuracy_m);
        days.push(DailyLocationSummary {
            date,
            summary: summarize(&kept, excluded),
        });
        kept_by_day.push(kept);
    }
    let total = sum_summaries(days.iter().map(|day| &day.summary));

    let segments = split_segments(kept_by_day.into_iter().flatten());
    let original_points = count_points(&segments);
    let segments = simplify_segments(segments, max_track_points);
    let returned_points = count_points(&segments);

    LocationHistory {
        from,
        to,
        timezone,
        total,
        days,
        segments,
        original_points,
        returned_points,
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

    /// 受け付けられない期間は、リポジトリを読まずに理由を返すことを確認する。
    ///
    /// 呼び出し側が確かめ忘れても、端の日付でパニックしたり逆向きの期間を空の結果にしたりしないため。
    #[tokio::test]
    async fn browse_rejects_invalid_ranges_without_reading_the_repository() {
        let ymd = |year, month, day| NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let cases = [
            (date(2), date(1), HistoryRangeError::Reversed),
            (
                ymd(2016, 1, 1),
                ymd(2016, 1, 1) + chrono::Days::new(MAX_HISTORY_RANGE_DAYS as u64),
                HistoryRangeError::TooLong,
            ),
            (
                NaiveDate::MAX,
                NaiveDate::MAX,
                HistoryRangeError::OutOfRange,
            ),
            (
                NaiveDate::MIN,
                NaiveDate::MIN,
                HistoryRangeError::OutOfRange,
            ),
            (
                ymd(0, 12, 31),
                ymd(0, 12, 31),
                HistoryRangeError::OutOfRange,
            ),
            (
                ymd(10000, 1, 1),
                ymd(10000, 1, 1),
                HistoryRangeError::OutOfRange,
            ),
        ];

        for (from, to, expected) in cases {
            let mut repo = MockLocationRepository::new();
            repo.expect_locations_between().never();

            let error = use_case(repo, 100).browse(from, to).await.unwrap_err();

            assert!(
                matches!(error, BrowseLocationHistoryError::InvalidRange(reason) if reason == expected),
                "{from}..{to}: {error:?}"
            );
        }
    }

    /// 開始日と終了日の差が上限の 1 日手前までなら受け付けることを確認する。
    #[tokio::test]
    async fn browse_accepts_the_longest_allowed_range() {
        let from = NaiveDate::from_ymd_opt(2016, 1, 1).unwrap();
        let to = from + chrono::Days::new(MAX_HISTORY_RANGE_DAYS as u64 - 1);
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .times(1)
            .returning(|_, _| Ok(Vec::new()));

        let history = use_case(repo, 100).browse(from, to).await.unwrap();

        assert_eq!(history.days.len(), MAX_HISTORY_RANGE_DAYS as usize);
    }

    /// リポジトリの失敗は、期間の誤りと区別して返すことを確認する。
    #[tokio::test]
    async fn browse_reports_repository_errors_as_failures() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .returning(|_, _| Err(anyhow::anyhow!("database is gone")));

        let error = use_case(repo, 100)
            .browse(date(1), date(1))
            .await
            .unwrap_err();

        assert!(
            matches!(error, BrowseLocationHistoryError::Failed(_)),
            "{error:?}"
        );
    }
}
