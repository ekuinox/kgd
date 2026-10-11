//! 日付からの日報の取得（find_by_date）のテスト。

use chrono::NaiveDate;
use mockall::predicate::eq;

use crate::test_support::{entry, utc};

use super::*;

/// 今の表現 (タイムゾーン基準の暦日 0 時) で保存された日報が見つかれば、
/// 古い表現では引かずに(times(0))それを返すことを確認する。
#[tokio::test]
async fn find_by_date_returns_entry_stored_at_local_midnight() {
    let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
    // 2026-09-28 00:00 JST
    let local_midnight = utc(2026, 9, 27, 15, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .with(eq(local_midnight))
        .times(1)
        .returning(move |_| Ok(Some(entry(100, local_midnight))));
    repo.expect_get_by_date()
        .with(eq(utc(2026, 9, 28, 0, 0)))
        .times(0);

    let l = lifecycle_with(
        calendar,
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        MockClock::new(),
    );
    let found = l
        .find_by_date(NaiveDate::from_ymd_opt(2026, 9, 28).unwrap())
        .await
        .unwrap();

    assert_eq!(found.map(|e| e.thread_id), Some(100));
}

/// 今の表現で見つからなければ、マイグレーション前の古い表現 (UTC の 0 時) で
/// 保存された日報を引くことを確認する。
#[tokio::test]
async fn find_by_date_falls_back_to_legacy_utc_midnight() {
    let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
    let legacy = utc(2025, 1, 20, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .with(eq(utc(2025, 1, 19, 15, 0)))
        .times(1)
        .returning(|_| Ok(None));
    repo.expect_get_by_date()
        .with(eq(legacy))
        .times(1)
        .returning(move |_| Ok(Some(entry(200, legacy))));

    let l = lifecycle_with(
        calendar,
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        MockClock::new(),
    );
    let found = l
        .find_by_date(NaiveDate::from_ymd_opt(2025, 1, 20).unwrap())
        .await
        .unwrap();

    assert_eq!(found.map(|e| e.thread_id), Some(200));
}

/// どちらの表現でも見つからなければ None を返すことを確認する。
#[tokio::test]
async fn find_by_date_returns_none_when_no_entry() {
    let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(2).returning(|_| Ok(None));

    let l = lifecycle_with(
        calendar,
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        MockClock::new(),
    );
    let found = l
        .find_by_date(NaiveDate::from_ymd_opt(2026, 9, 28).unwrap())
        .await
        .unwrap();

    assert!(found.is_none());
}
