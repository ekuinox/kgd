//! 日報作成（create_or_reopen）のテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, fixed_clock, utc};

use super::*;

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
