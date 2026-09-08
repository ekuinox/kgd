//! 日報作成（create_or_reopen）のテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, fixed_clock, utc};

use super::*;

/// 今日の日報が既に存在する場合、Notion 呼び出しやスレッド新規作成を行わず(times(0))、
/// 既存スレッドを reopen してボタン有無を確認し、Reopened を返すことを確認する。
#[tokio::test]
async fn create_reopens_existing_thread() {
    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .with(eq(today))
        .times(1)
        .returning(move |_| Ok(Some(entry(100, today))));
    let notion = MockNotionApi::new(); // Notion は呼ばれない
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_reopen_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(true));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    gateway.expect_create_diary_forum_post().times(0);
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);
    let outcome = l.create_or_reopen().await.unwrap();

    assert_eq!(outcome, DiaryCreateOutcome::Reopened { thread_id: 100 });
}

/// day_start_hour より前（深夜）に日報作成を実行した場合、新しいスレッドを作らず(times(0))
/// 同じ日報日にあたる既存スレッドを再開することを確認する。
/// 早出しした日報が無いので、次の日報日は再開の対象にならない。
#[tokio::test]
async fn create_reopens_the_same_diary_day_before_day_start_hour() {
    let calendar = DiaryCalendar::new(chrono_tz::Asia::Tokyo, 7);
    // 2026-08-10 03:00 JST。day_start_hour(7時)より前なので日報日は 2026-08-09
    let now = utc(2026, 8, 9, 18, 0);
    // 2026-08-09 00:00 JST
    let diary_day = utc(2026, 8, 8, 15, 0);
    // 2026-08-10 00:00 JST
    let next_day = utc(2026, 8, 9, 15, 0);

    let mut repo = MockDiaryRepository::new();
    // 早出しした日報は無い
    repo.expect_get_by_date()
        .with(eq(next_day))
        .times(1)
        .returning(|_| Ok(None));
    repo.expect_get_by_date()
        .with(eq(diary_day))
        .times(1)
        .returning(move |_| Ok(Some(entry(100, diary_day))));
    let notion = MockNotionApi::new(); // Notion は呼ばれない
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_reopen_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(true));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    gateway.expect_create_diary_forum_post().times(0);

    let l = lifecycle_with(calendar, repo, notion, gateway, fixed_clock(now));
    let outcome = l.create_or_reopen().await.unwrap();

    assert_eq!(outcome, DiaryCreateOutcome::Reopened { thread_id: 100 });
}

/// 今日の日報が無く、同名の Notion ページが既存の場合、create_diary_page を呼ばず(times(0))
/// 既存ページを再利用してスレッドを作成・登録し、reused_page=true の Created を返すことを確認する。
#[tokio::test]
async fn create_makes_new_thread_with_existing_page() {
    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    repo.expect_insert()
        .withf(move |e| e.thread_id == 200 && e.page_id == "page-x" && e.date == today)
        .times(1)
        .returning(|_| Ok(()));
    let mut notion = MockNotionApi::new();
    // 既存ページが見つかる → create は呼ばれない
    notion
        .expect_find_diary_page_by_title()
        .with(eq("2025-01-02"))
        .times(1)
        .returning(|_| {
            Ok(Some((
                "page-x".to_string(),
                "https://notion.example/x".to_string(),
            )))
        });
    notion.expect_create_diary_page().times(0);
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .with(eq(555u64), eq("2025-01-02"), eq("https://notion.example/x"))
        .times(1)
        .returning(|_, _, _| Ok(200));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);
    let outcome = l.create_or_reopen().await.unwrap();

    assert_eq!(
        outcome,
        DiaryCreateOutcome::Created {
            thread_id: 200,
            page_url: "https://notion.example/x".to_string(),
            reused_page: true,
        }
    );
}

/// 日報作成は早出しの起点にしない。早出しの窓でも日報が一つも無ければ、
/// 次の日報日ではなく今の日報日で作ることを確認する。
#[tokio::test]
async fn create_does_not_start_the_next_day_early() {
    let calendar = DiaryCalendar::new(chrono_tz::UTC, 9);
    // 2025-01-02 03:00 UTC。day_start_hour(9時)より前なので日報日は 2025-01-01
    let now = utc(2025, 1, 2, 3, 0);

    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(2).returning(|_| Ok(None));
    repo.expect_insert()
        .withf(|entry| entry.date == utc(2025, 1, 1, 0, 0))
        .times(1)
        .returning(|_| Ok(()));
    let mut notion = MockNotionApi::new();
    notion
        .expect_find_diary_page_by_title()
        .with(eq("2025-01-01".to_string()))
        .times(1)
        .returning(|_| Ok(None));
    notion
        .expect_create_diary_page()
        .with(eq("2025-01-01".to_string()))
        .times(1)
        .returning(|_| Ok(("p".to_string(), "u".to_string())));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .withf(|_, title, _| title == "2025-01-01")
        .times(1)
        .returning(|_, _, _| Ok(201));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));

    let l = lifecycle_with(calendar, repo, notion, gateway, fixed_clock(now));
    let outcome = l.create_or_reopen().await.unwrap();

    assert!(matches!(
        outcome,
        DiaryCreateOutcome::Created { thread_id: 201, .. }
    ));
}

/// 早出しで次の日報日の日報を作ってあれば、深夜の日報作成は前日のスレッドではなく
/// 早出しした方を再開することを確認する。
#[tokio::test]
async fn create_reopens_early_started_next_day() {
    let calendar = DiaryCalendar::new(chrono_tz::UTC, 9);
    // 2025-01-02 03:00 UTC。day_start_hour(9時)より前なので日報日は 2025-01-01
    let now = utc(2025, 1, 2, 3, 0);
    let next_day = utc(2025, 1, 2, 0, 0);

    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .with(eq(next_day))
        .times(1)
        .returning(move |_| Ok(Some(entry(400, next_day))));
    let notion = MockNotionApi::new(); // Notion は呼ばれない
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_reopen_thread()
        .with(eq(400u64))
        .times(1)
        .returning(|_| Ok(true));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    gateway.expect_create_diary_forum_post().times(0);

    let l = lifecycle_with(calendar, repo, notion, gateway, fixed_clock(now));
    let outcome = l.create_or_reopen().await.unwrap();

    assert_eq!(outcome, DiaryCreateOutcome::Reopened { thread_id: 400 });
}

/// 今日の日報も既存ページも無い場合、新規 Notion ページを作成してスレッドを作成し、
/// ボタンが無ければ send_close_and_new_button を送り、reused_page=false の Created を返すことを確認する。
#[tokio::test]
async fn create_makes_new_page_when_not_found() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    repo.expect_insert().times(1).returning(|_| Ok(()));
    let mut notion = MockNotionApi::new();
    notion
        .expect_find_diary_page_by_title()
        .times(1)
        .returning(|_| Ok(None));
    notion
        .expect_create_diary_page()
        .with(eq("2025-01-02"))
        .times(1)
        .returning(|_| {
            Ok((
                "page-new".to_string(),
                "https://notion.example/new".to_string(),
            ))
        });
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .times(1)
        .returning(|_, _, _| Ok(201));
    // ボタンが無ければ送信する
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(false));
    gateway
        .expect_send_close_and_new_button()
        .with(eq(201u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);
    let outcome = l.create_or_reopen().await.unwrap();

    assert_eq!(
        outcome,
        DiaryCreateOutcome::Created {
            thread_id: 201,
            page_url: "https://notion.example/new".to_string(),
            reused_page: false,
        }
    );
}
