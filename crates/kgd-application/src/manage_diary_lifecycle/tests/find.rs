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

/// 今の表現で見つからなければ、古い表現 (UTC の 0 時) で保存された日報を引くことを確認する。
#[tokio::test]
async fn find_by_date_falls_back_to_legacy_utc_midnight() {
    let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 8);
    let legacy = utc(2026, 1, 26, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        // 2026-01-26 00:00 JST
        .with(eq(utc(2026, 1, 25, 15, 0)))
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
        .find_by_date(NaiveDate::from_ymd_opt(2026, 1, 26).unwrap())
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

/// 暦から消えた日を指定しても、パニックせずに古い表現だけを引いて None を返すことを確認する。
///
/// 日付はユーザーの入力に由来するため。Pacific/Apia では 2011-12-30 が存在しない。
#[tokio::test]
async fn find_by_date_returns_none_for_a_skipped_day() {
    let calendar = DiaryCalendar::new(chrono_tz::Pacific::Apia, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .with(eq(utc(2011, 12, 30, 0, 0)))
        .times(1)
        .returning(|_| Ok(None));

    let l = lifecycle_with(
        calendar,
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        MockClock::new(),
    );
    let found = l
        .find_by_date(NaiveDate::from_ymd_opt(2011, 12, 30).unwrap())
        .await
        .unwrap();

    assert!(found.is_none());
}
