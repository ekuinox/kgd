//! 日報日の位置ログから地図画像と集計値を作るユースケース。

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::{DateTime, NaiveDate, Utc};

use kgd_domain::{
    DiaryCalendar, LocationSummary, filter_accurate, fit_viewport, split_segments, summarize,
};

use super::ports::{LocationRepository, MapRenderer};

/// レポートの作り方の設定。
#[derive(Debug, Clone, Copy)]
pub struct LocationReportSettings {
    /// 日報日の区切り方
    pub calendar: DiaryCalendar,
    /// これを超える水平精度 (メートル) の点を除く
    pub max_accuracy_m: i32,
    /// 地図画像の幅 (ピクセル)
    pub image_width: u32,
    /// 地図画像の高さ (ピクセル)
    pub image_height: u32,
}

/// 日報日 1 日ぶんの位置ログのレポート。
#[derive(Debug, Clone, PartialEq)]
pub struct LocationReport {
    /// 対象の日報日
    pub date: NaiveDate,
    /// 集計に使った範囲 (開始を含み終了を含まない)
    pub range: (DateTime<Utc>, DateTime<Utc>),
    /// 集計値
    pub summary: LocationSummary,
    /// 地図画像 (PNG)。集計に使える点が無ければ None
    pub image: Option<Vec<u8>>,
}

/// 日報日の位置ログから地図画像と集計値を作るユースケース。
///
/// 届け先 (日報、スラッシュコマンド、将来の CLI) を知らず、レポートを返すだけにする。
pub struct BuildLocationReportUseCase {
    /// 位置情報リポジトリポート
    repo: Arc<dyn LocationRepository>,
    /// 地図描画ポート
    renderer: Arc<dyn MapRenderer>,
    /// レポートの作り方の設定
    settings: LocationReportSettings,
}

impl BuildLocationReportUseCase {
    /// 新しい BuildLocationReportUseCase を作成する。
    pub fn new(
        repo: Arc<dyn LocationRepository>,
        renderer: Arc<dyn MapRenderer>,
        settings: LocationReportSettings,
    ) -> Self {
        Self {
            repo,
            renderer,
            settings,
        }
    }

    /// 指定した日報日のレポートを作る。
    ///
    /// `until` を渡すと範囲の終わりをそこで切り詰める (日報日の始まりより前にはならない)。
    pub async fn build(
        &self,
        date: NaiveDate,
        until: Option<DateTime<Utc>>,
    ) -> Result<LocationReport> {
        let (start, day_end) = self
            .settings
            .calendar
            .day_range(date)
            .with_context(|| format!("diary date {date} is out of the representable range"))?;
        let end = until.map_or(day_end, |until| until.clamp(start, day_end));

        let raw = if end > start {
            self.repo.locations_between(start, end).await?
        } else {
            Vec::new()
        };
        let (points, excluded) = filter_accurate(raw, self.settings.max_accuracy_m);
        let summary = summarize(&points, excluded);

        let image = match fit_viewport(
            &points,
            self.settings.image_width,
            self.settings.image_height,
        ) {
            Some(viewport) => Some(
                self.renderer
                    .render(&viewport, &split_segments(points))
                    .await?,
            ),
            None => None,
        };

        Ok(LocationReport {
            date,
            range: (start, end),
            summary,
            image,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use kgd_domain::TrackPoint;

    use crate::ports::{MockLocationRepository, MockMapRenderer};

    use super::*;

    /// Asia/Tokyo の時刻を UTC で作る。
    fn jst(day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        chrono_tz::Asia::Tokyo
            .with_ymd_and_hms(2026, 9, day, hour, min, 0)
            .unwrap()
            .to_utc()
    }

    /// 精度を指定した点を作る。
    fn point(at: DateTime<Utc>, accuracy_m: i32) -> TrackPoint {
        TrackPoint {
            at,
            lat: 35.68,
            lon: 139.76,
            accuracy_m: Some(accuracy_m),
            activity: None,
        }
    }

    fn settings() -> LocationReportSettings {
        LocationReportSettings {
            calendar: DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8),
            max_accuracy_m: 200,
            image_width: 1024,
            image_height: 1024,
        }
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 28).unwrap()
    }

    /// 日報日の範囲 (開始を含み終了を含まない) をそのままリポジトリへ渡すことを確認する。
    #[tokio::test]
    async fn build_queries_the_half_open_diary_day_range() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(29, 8, 0))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let mut renderer = MockMapRenderer::new();
        renderer.expect_render().times(0);
        let use_case =
            BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.range, (jst(28, 8, 0), jst(29, 8, 0)));
        assert_eq!(report.image, None);
        assert_eq!(report.summary.point_count, 0);
    }

    /// until を渡すと範囲の終わりがそこで切り詰められることを確認する。
    ///
    /// スラッシュコマンドで進行中の日報日を「今まで」の分だけ見るため。
    #[tokio::test]
    async fn build_truncates_the_range_at_until() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(28, 14, 32))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(28, 14, 32))).await.unwrap();

        assert_eq!(report.range.1, jst(28, 14, 32));
    }

    /// until が日報日の終わりより後なら、日報日の終わりで止めることを確認する。
    #[tokio::test]
    async fn build_keeps_day_end_when_until_is_later() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|_, end| *end == jst(29, 8, 0))
            .times(1)
            .returning(|_, _| Ok(vec![]));
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(30, 0, 0))).await.unwrap();

        assert_eq!(report.range.1, jst(29, 8, 0));
    }

    /// until が日報日の始まりより前なら、リポジトリを呼ばずに空のレポートを返すことを確認する。
    #[tokio::test]
    async fn build_skips_query_when_until_precedes_day_start() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().times(0);
        let use_case = BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            settings(),
        );

        let report = use_case.build(date(), Some(jst(28, 7, 0))).await.unwrap();

        assert_eq!(report.range, (jst(28, 8, 0), jst(28, 8, 0)));
        assert_eq!(report.summary.point_count, 0);
    }

    /// 精度不足の点を除いて集計し、残った点で地図を描くことを確認する。
    #[tokio::test]
    async fn build_renders_map_from_accurate_points() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .returning(|_, _| Ok(vec![point(jst(28, 9, 0), 10), point(jst(28, 9, 1), 1414)]));
        let mut renderer = MockMapRenderer::new();
        renderer
            .expect_render()
            .withf(|viewport, segments| {
                viewport.width == 1024 && segments.len() == 1 && segments[0].points.len() == 1
            })
            .times(1)
            .returning(|_, _| Ok(vec![1, 2, 3]));
        let use_case =
            BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.image, Some(vec![1, 2, 3]));
        assert_eq!(report.summary.point_count, 1);
        assert_eq!(report.summary.excluded_count, 1);
    }

    /// 点がすべて精度不足で除外されたら、地図を描かず画像を持たないことを確認する。
    #[tokio::test]
    async fn build_returns_no_image_when_all_points_are_inaccurate() {
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .returning(|_, _| Ok(vec![point(jst(28, 9, 0), 1414), point(jst(28, 9, 1), 900)]));
        let mut renderer = MockMapRenderer::new();
        renderer.expect_render().times(0);
        let use_case =
            BuildLocationReportUseCase::new(Arc::new(repo), Arc::new(renderer), settings());

        let report = use_case.build(date(), None).await.unwrap();

        assert_eq!(report.image, None);
        assert_eq!(report.summary.point_count, 0);
        assert_eq!(report.summary.excluded_count, 2);
    }
}
