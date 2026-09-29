//! 前の日報日の位置ログのレポートを、その日の日報へ載せる定時ジョブ。

use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use chrono_tz::Tz;
use tracing::info;

use kgd_domain::{
    DiaryCalendar, DiaryPost, DiaryPostImage, format_empty_location_report, format_location_report,
};

use super::{
    BuildLocationReportUseCase, LocationReport, PublishDiaryPostUseCase, ScheduledJob, ports::Clock,
};

/// 前の日報日の位置ログのレポートを、その日の日報へ載せる定時ジョブ。
///
/// 対象は常に「1 つ前の日報日」だけとし、完了するまで tick ごとに再試行する。
pub struct DailyLocationReportJob {
    /// レポートを作るユースケース
    build: Arc<BuildLocationReportUseCase>,
    /// 日報へ載せるユースケース
    publish: Arc<PublishDiaryPostUseCase>,
    /// 時刻ポート
    clock: Arc<dyn Clock>,
    /// 日報日の区切り方
    calendar: DiaryCalendar,
}

impl DailyLocationReportJob {
    /// 新しい DailyLocationReportJob を作成する。
    pub fn new(
        build: Arc<BuildLocationReportUseCase>,
        publish: Arc<PublishDiaryPostUseCase>,
        clock: Arc<dyn Clock>,
        calendar: DiaryCalendar,
    ) -> Self {
        Self {
            build,
            publish,
            clock,
            calendar,
        }
    }
}

#[async_trait::async_trait]
impl ScheduledJob for DailyLocationReportJob {
    fn name(&self) -> &'static str {
        "daily_location_report"
    }

    async fn tick(&self) -> Result<()> {
        let date = self.calendar.previous_date(self.clock.now());
        let key = location_report_key(date);
        if self.publish.is_done(&key).await? {
            return Ok(());
        }

        let report = self.build.build(date, None).await?;
        let post = to_diary_post(key, &report, *self.calendar.timezone());
        let outcome = self.publish.publish(post).await?;
        info!(%date, ?outcome, "Daily location report handled");
        Ok(())
    }
}

/// 日次レポートの投稿キーを返す。
fn location_report_key(date: NaiveDate) -> String {
    format!("location-report:{}", date.format("%Y-%m-%d"))
}

/// レポートを日報への投稿に変換する。画像が無ければ「記録なし」の本文だけにする。
fn to_diary_post(key: String, report: &LocationReport, timezone: Tz) -> DiaryPost {
    match &report.image {
        Some(png) => DiaryPost {
            key,
            date: report.date,
            text: format_location_report(report.date, report.range, &report.summary, timezone)
                .to_plain_text(),
            images: vec![DiaryPostImage {
                filename: format!("location-{}.png", report.date.format("%Y-%m-%d")),
                content_type: "image/png".to_string(),
                bytes: png.clone(),
            }],
        },
        None => DiaryPost {
            key,
            date: report.date,
            text: format_empty_location_report(report.date),
            images: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeDelta, TimeZone as _, Utc};

    use kgd_domain::{DiaryPostRecord, LocationSummary};

    use crate::{
        LocationReportSettings,
        ports::{
            MockDiaryPostRepository, MockDiaryRepository, MockDiscordGateway,
            MockLocationRepository, MockMapRenderer, MockNotionApi,
        },
        test_support::{entry, fixed_clock},
    };

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

    fn build_use_case(repo: MockLocationRepository) -> Arc<BuildLocationReportUseCase> {
        Arc::new(BuildLocationReportUseCase::new(
            Arc::new(repo),
            Arc::new(MockMapRenderer::new()),
            LocationReportSettings {
                calendar: calendar(),
                max_accuracy_m: 200,
                image_width: 1024,
                image_height: 1024,
            },
        ))
    }

    fn publish_use_case(
        diary: MockDiaryRepository,
        posts: MockDiaryPostRepository,
        notion: MockNotionApi,
        discord: MockDiscordGateway,
    ) -> Arc<PublishDiaryPostUseCase> {
        Arc::new(PublishDiaryPostUseCase::new(
            Arc::new(diary),
            Arc::new(posts),
            Arc::new(notion),
            Arc::new(discord),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        ))
    }

    /// 前の日報日の投稿が済んでいれば、レポートを作らずに終えることを確認する。
    ///
    /// 毎分の tick で地図を描き直さないため。
    #[tokio::test]
    async fn tick_skips_building_when_already_done() {
        let mut posts = MockDiaryPostRepository::new();
        posts
            .expect_get()
            .withf(|key| key == "location-report:2026-09-28")
            .returning(|key| {
                Ok(Some(DiaryPostRecord {
                    key: key.to_string(),
                    diary_date: NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
                    notion_posted_at: None,
                    thread_message_id: None,
                    thread_posted_at: None,
                    skipped_at: Some(jst(29, 8, 1)),
                }))
            });
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between().times(0);
        let job = DailyLocationReportJob::new(
            build_use_case(repo),
            publish_use_case(
                MockDiaryRepository::new(),
                posts,
                MockNotionApi::new(),
                MockDiscordGateway::new(),
            ),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        );

        job.tick().await.unwrap();
    }

    /// 記録が無い日は「記録なし」の本文だけを、画像なしで日報へ載せることを確認する。
    #[tokio::test]
    async fn tick_publishes_empty_report_when_no_points() {
        let mut posts = MockDiaryPostRepository::new();
        posts.expect_get().times(2).returning(|_| Ok(None));
        posts
            .expect_mark_notion_posted()
            .returning(|_, _, _| Ok(()));
        posts
            .expect_mark_thread_posted()
            .returning(|_, _, _, _| Ok(()));
        let mut repo = MockLocationRepository::new();
        repo.expect_locations_between()
            .withf(|start, end| *start == jst(28, 8, 0) && *end == jst(29, 8, 0))
            .returning(|_, _| Ok(vec![]));
        let mut diary = MockDiaryRepository::new();
        diary
            .expect_get_by_date()
            .returning(|d| Ok(Some(entry(10, d))));
        let mut notion = MockNotionApi::new();
        notion.expect_upload_file().times(0);
        notion
            .expect_append_blocks()
            .withf(|_, children| {
                children.len() == 1
                    && children[0]["paragraph"]["rich_text"][0]["text"]["content"]
                        == "位置ログ 2026-09-28 記録なし"
            })
            .times(1)
            .returning(|_, _| Ok(vec!["b1".to_string()]));
        let mut discord = MockDiscordGateway::new();
        discord.expect_thread_state().returning(|_| {
            Ok(Some(kgd_domain::ThreadState {
                is_public_thread: true,
                archived: false,
                locked: false,
            }))
        });
        discord
            .expect_send_text_with_images()
            .withf(|_, content, images| {
                content == "位置ログ 2026-09-28 記録なし" && images.is_empty()
            })
            .times(1)
            .returning(|_, _, _| Ok(555));
        let job = DailyLocationReportJob::new(
            build_use_case(repo),
            publish_use_case(diary, posts, notion, discord),
            Arc::new(fixed_clock(jst(29, 9, 0))),
            calendar(),
        );

        job.tick().await.unwrap();
    }

    /// 画像のあるレポートは、本文と PNG 1 枚の投稿に変換されることを確認する。
    #[test]
    fn to_diary_post_attaches_png_with_report_text() {
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
            image: Some(vec![1, 2, 3]),
        };

        let post = to_diary_post(location_report_key(date), &report, chrono_tz::Asia::Tokyo);

        assert_eq!(post.key, "location-report:2026-09-28");
        assert_eq!(post.date, date);
        assert!(
            post.text
                .starts_with("位置ログ 2026-09-28 (08:00〜翌 08:00)\n")
        );
        assert!(post.text.ends_with("© OpenStreetMap contributors"));
        assert_eq!(post.images.len(), 1);
        assert_eq!(post.images[0].filename, "location-2026-09-28.png");
        assert_eq!(post.images[0].content_type, "image/png");
        assert_eq!(post.images[0].bytes, vec![1, 2, 3]);
    }
}
