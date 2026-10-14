//! クローズ & 新規作成・クローズのテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, fixed_clock, utc};

use super::*;

/// スレッドが日報として未登録（get_by_thread が None）の場合、
/// precheck が NotDiaryThread を返すことを確認する。
#[tokio::test]
async fn precheck_detects_non_diary_thread() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(None));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);
    let precheck = l.close_and_new_precheck(100).await.unwrap();

    assert_eq!(precheck, CloseAndNewPrecheck::NotDiaryThread);
}

/// 既存の日報スレッドに対する precheck で、自分自身が最新日付なら AlreadyLatest を、
/// 別スレッドが最新なら LatestExists{thread_id} を返すことを確認する。
#[tokio::test]
async fn precheck_detects_latest_thread() {
    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .times(2)
        .returning(move |id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
    repo.expect_get_by_date()
        .times(2)
        .returning(move |_| Ok(Some(entry(300, today))));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);

    // 自分自身が最新
    assert_eq!(
        l.close_and_new_precheck(300).await.unwrap(),
        CloseAndNewPrecheck::AlreadyLatest
    );
    // 別スレッドが最新
    assert_eq!(
        l.close_and_new_precheck(100).await.unwrap(),
        CloseAndNewPrecheck::LatestExists { thread_id: 300 }
    );
}

/// 日報スレッドから close_and_create_new した場合、新スレッドを作成・登録した後、
/// 旧スレッド(100)へ新スレッド(<#400>)への案内を送ってからクローズすることを確認する。
#[tokio::test]
async fn close_and_create_new_mentions_then_closes_old_thread() {
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
        .times(1)
        .returning(|_| Ok(("p".to_string(), "u".to_string())));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .times(1)
        .returning(|_, _, _| Ok(400));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    // 旧スレッドへの案内 → クローズの順
    gateway
        .expect_send_text()
        .withf(|channel_id, content| *channel_id == 100 && content.contains("<#400>"))
        .times(1)
        .returning(|_, _| Ok(901));
    gateway
        .expect_close_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);
    let outcome = l.close_and_create_new(100).await.unwrap();

    assert_eq!(outcome, CloseAndNewOutcome::Created { thread_id: 400 });
}

/// 書き込み用チャンネル(500)から close_and_create_new した場合、最新エントリの
/// スレッド(100)をクローズ対象にしつつ、案内はボタンが押されたチャンネル(500)へ送ることを確認する。
#[tokio::test]
async fn close_and_create_new_from_write_channel_closes_latest_thread() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    // 書き込み用チャンネル経由では最新エントリのスレッドをクローズ対象にする
    repo.expect_get_latest_entry()
        .times(1)
        .returning(|| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
    repo.expect_insert().times(1).returning(|_| Ok(()));
    let mut notion = MockNotionApi::new();
    notion
        .expect_find_diary_page_by_title()
        .times(1)
        .returning(|_| Ok(None));
    notion
        .expect_create_diary_page()
        .times(1)
        .returning(|_| Ok(("p".to_string(), "u".to_string())));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .times(1)
        .returning(|_, _, _| Ok(400));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    // 案内はボタンが押された書き込み用チャンネルへ送る
    gateway
        .expect_send_text()
        .withf(|channel_id, content| *channel_id == 500 && content.contains("<#400>"))
        .times(1)
        .returning(|_, _| Ok(902));
    gateway
        .expect_close_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);
    let outcome = l.close_and_create_new(500).await.unwrap();

    assert_eq!(outcome, CloseAndNewOutcome::Created { thread_id: 400 });
}

/// 書き込み用チャンネル(500)からの precheck では get_by_thread を呼ばず(times(0))、
/// 今日の日付の日報が無ければ ReadyToCreate を返すことを確認する。
#[tokio::test]
async fn precheck_allows_write_channel_without_thread_registration() {
    let mut repo = MockDiaryRepository::new();
    // 書き込み用チャンネルでは get_by_thread を確認しない
    repo.expect_get_by_thread().times(0);
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, MockNotionApi::new(), MockDiscordGateway::new(), clock);
    let precheck = l.close_and_new_precheck(500).await.unwrap();

    assert_eq!(precheck, CloseAndNewPrecheck::ReadyToCreate);
}

/// day_start_hour より前（早出しの窓）の precheck は、今の日報日ではなく
/// 暦日にあたる次の日報日を見て、そこに日報が無ければ ReadyToCreate を返すことを確認する。
#[tokio::test]
async fn precheck_targets_next_day_before_day_start_hour() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .times(1)
        .returning(|id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
    // 今の日報日(1/1)ではなく次の日報日(1/2)を確認する
    repo.expect_get_by_date()
        .withf(|date| *date == utc(2025, 1, 2, 0, 0))
        .times(1)
        .returning(|_| Ok(None));
    let clock = fixed_clock(utc(2025, 1, 2, 3, 0));

    let l = lifecycle_with(
        DiaryCalendar::new(chrono_tz::UTC, 9),
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        clock,
    );

    assert_eq!(
        l.close_and_new_precheck(100).await.unwrap(),
        CloseAndNewPrecheck::ReadyToCreate
    );
}

/// 早出しで次の日報日の日報を既に作っていれば、もう一度ボタンを押しても
/// 新規作成へ進まず AlreadyLatest を返すことを確認する。
#[tokio::test]
async fn precheck_detects_already_early_started_next_day() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .times(1)
        .returning(|id| Ok(Some(entry(id, utc(2025, 1, 2, 0, 0)))));
    repo.expect_get_by_date()
        .withf(|date| *date == utc(2025, 1, 2, 0, 0))
        .times(1)
        .returning(|_| Ok(Some(entry(400, utc(2025, 1, 2, 0, 0)))));
    let clock = fixed_clock(utc(2025, 1, 2, 3, 0));

    let l = lifecycle_with(
        DiaryCalendar::new(chrono_tz::UTC, 9),
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        clock,
    );

    assert_eq!(
        l.close_and_new_precheck(400).await.unwrap(),
        CloseAndNewPrecheck::AlreadyLatest
    );
}

/// day_start_hour 以降は早出しの窓の外なので、precheck が従来どおり
/// 今の日報日を見ることを確認する。
#[tokio::test]
async fn precheck_targets_today_after_day_start_hour() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .times(1)
        .returning(|id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
    repo.expect_get_by_date()
        .withf(|date| *date == utc(2025, 1, 2, 0, 0))
        .times(1)
        .returning(|_| Ok(None));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle_with(
        DiaryCalendar::new(chrono_tz::UTC, 9),
        repo,
        MockNotionApi::new(),
        MockDiscordGateway::new(),
        clock,
    );

    assert_eq!(
        l.close_and_new_precheck(100).await.unwrap(),
        CloseAndNewPrecheck::ReadyToCreate
    );
}

/// 早出しの窓で close_and_create_new すると、次の日報日の日付で
/// Notion ページと日報エントリを作ることを確認する。
#[tokio::test]
async fn close_and_create_new_uses_next_day_before_day_start_hour() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .withf(|date| *date == utc(2025, 1, 2, 0, 0))
        .times(1)
        .returning(|_| Ok(None));
    repo.expect_insert()
        .withf(|entry| entry.date == utc(2025, 1, 2, 0, 0))
        .times(1)
        .returning(|_| Ok(()));
    let mut notion = MockNotionApi::new();
    notion
        .expect_find_diary_page_by_title()
        .with(eq("2025-01-02".to_string()))
        .times(1)
        .returning(|_| Ok(None));
    notion
        .expect_create_diary_page()
        .with(eq("2025-01-02".to_string()))
        .times(1)
        .returning(|_| Ok(("p".to_string(), "u".to_string())));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_create_diary_forum_post()
        .withf(|_, title, _| title == "2025-01-02")
        .times(1)
        .returning(|_, _, _| Ok(400));
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    gateway
        .expect_send_text()
        .times(1)
        .returning(|_, _| Ok(901));
    gateway
        .expect_close_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 3, 0));

    let l = lifecycle_with(
        DiaryCalendar::new(chrono_tz::UTC, 9),
        repo,
        notion,
        gateway,
        clock,
    );

    assert_eq!(
        l.close_and_create_new(100).await.unwrap(),
        CloseAndNewOutcome::Created { thread_id: 400 }
    );
}

/// 日報として未登録のスレッドを close した場合、close_thread を呼ばず(times(0))、
/// NotDiaryThread を返すことを確認する。
#[tokio::test]
async fn close_skips_non_diary_thread() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread().times(1).returning(|_| Ok(None));
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_close_thread().times(0);
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, MockNotionApi::new(), gateway, clock);
    let outcome = l.close(100).await.unwrap();

    assert_eq!(outcome, DiaryCloseOutcome::NotDiaryThread);
}

/// 日報として登録済みのスレッド(100)を close した場合、close_thread を呼び出して
/// Closed を返すことを確認する。
#[tokio::test]
async fn close_closes_diary_thread() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .times(1)
        .returning(|id| Ok(Some(entry(id, utc(2025, 1, 1, 0, 0)))));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_close_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, MockNotionApi::new(), gateway, clock);
    let outcome = l.close(100).await.unwrap();

    assert_eq!(outcome, DiaryCloseOutcome::Closed);
}

/// ロックを取るまでの間に別の押下が同じ日報日のスレッドを作り終えていた場合、
/// 作成へ進まず(create_diary_forum_post / insert を呼ばず) AlreadyExists を返すことを確認する。
#[tokio::test]
async fn close_and_create_new_skips_when_entry_already_created() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .times(1)
        .returning(|_| Ok(Some(entry(400, utc(2025, 1, 2, 0, 0)))));
    repo.expect_insert().times(0);
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_create_diary_forum_post().times(0);
    gateway.expect_send_text().times(0);
    gateway.expect_close_thread().times(0);
    let mut notion = MockNotionApi::new();
    notion.expect_find_diary_page_by_title().times(0);
    notion.expect_create_diary_page().times(0);
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = lifecycle(repo, notion, gateway, clock);

    assert_eq!(
        l.close_and_create_new(100).await.unwrap(),
        CloseAndNewOutcome::AlreadyExists { thread_id: 400 }
    );
}

/// ボタンを短い間隔で二度押して close_and_create_new が並行に走っても、
/// スレッドと日報エントリは一つだけ作られることを確認する。
// 二つの押下を本当に並行させるため、マルチスレッドのランタイムで動かす
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_and_create_new_serializes_concurrent_presses() {
    // 1 件目が insert するまでは get_by_date が None を返す状況を再現する
    let stored: Arc<std::sync::Mutex<Option<DiaryEntry>>> = Arc::new(std::sync::Mutex::new(None));

    let mut repo = MockDiaryRepository::new();
    let stored_for_get = Arc::clone(&stored);
    repo.expect_get_by_date()
        .returning(move |_| Ok(stored_for_get.lock().unwrap().clone()));
    let stored_for_insert = Arc::clone(&stored);
    repo.expect_insert().times(1).returning(move |entry| {
        *stored_for_insert.lock().unwrap() = Some(entry.clone());
        Ok(())
    });

    let mut notion = MockNotionApi::new();
    notion
        .expect_find_diary_page_by_title()
        .times(1)
        .returning(|_| Ok(Some(("p".to_string(), "u".to_string()))));

    let mut gateway = MockDiscordGateway::new();
    // スレッド作成に時間がかかる間に 2 件目の押下が追い付く状況を再現する
    gateway
        .expect_create_diary_forum_post()
        .times(1)
        .returning(|_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            Ok(400)
        });
    gateway
        .expect_has_close_and_new_button()
        .times(1)
        .returning(|_, _| Ok(true));
    gateway
        .expect_send_text()
        .times(1)
        .returning(|_, _| Ok(901));
    gateway.expect_close_thread().times(1).returning(|_| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 2, 9, 0));

    let l = Arc::new(lifecycle(repo, notion, gateway, clock));

    let first = tokio::spawn({
        let l = Arc::clone(&l);
        async move { l.close_and_create_new(100).await.unwrap() }
    });
    let second = tokio::spawn({
        let l = Arc::clone(&l);
        async move { l.close_and_create_new(100).await.unwrap() }
    });

    let outcomes = [first.await.unwrap(), second.await.unwrap()];

    assert!(outcomes.contains(&CloseAndNewOutcome::Created { thread_id: 400 }));
    assert!(outcomes.contains(&CloseAndNewOutcome::AlreadyExists { thread_id: 400 }));
}
