//! クローズ & 新規作成・クローズのテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, fixed_clock, utc};

use super::*;

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

#[tokio::test]
async fn close_and_create_new_mentions_then_closes_old_thread() {
    let mut repo = MockDiaryRepository::new();
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
    let new_thread_id = l.close_and_create_new(100).await.unwrap();

    assert_eq!(new_thread_id, 400);
}

#[tokio::test]
async fn close_and_create_new_from_write_channel_closes_latest_thread() {
    let mut repo = MockDiaryRepository::new();
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
    let new_thread_id = l.close_and_create_new(500).await.unwrap();

    assert_eq!(new_thread_id, 400);
}

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
