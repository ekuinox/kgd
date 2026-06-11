//! 書き込み用チャンネル投稿の転記処理のテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, text_sync_service, utc};

use super::*;

#[tokio::test]
async fn relay_syncs_posts_and_records_mapping() {
    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .times(1)
        .returning(move |_| Ok(Some(entry(100, today))));
    repo.expect_upsert_relayed_message()
        .withf(|relayed| {
            relayed.source_message_id == 10
                && relayed.thread_id == 100
                && relayed.relayed_message_id == 900
        })
        .times(1)
        .returning(|_| Ok(()));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_send_text()
        .withf(|thread_id, content| {
            *thread_id == 100
                && content.contains("hello")
                && content.contains("https://discord.com/channels/1/500/10")
        })
        .times(1)
        .returning(|_, _| Ok(900));
    gateway
        .expect_add_reaction()
        .with(eq(500u64), eq(10u64), eq("✅"))
        .times(1)
        .returning(|_, _, _| Ok(()));

    let relay = relay_use_case(repo, gateway, text_sync_service(1));
    relay.relay(&message(10, "hello")).await.unwrap();
}

#[tokio::test]
async fn relay_skips_when_no_diary_entry() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    repo.expect_get_latest_entry()
        .times(1)
        .returning(|| Ok(None));
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_send_text().times(0);

    let relay = relay_use_case(repo, gateway, text_sync_service(0));
    relay.relay(&message(10, "hello")).await.unwrap();
}

#[tokio::test]
async fn relay_skips_posting_for_empty_message() {
    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .times(1)
        .returning(move |_| Ok(Some(entry(100, today))));
    repo.expect_upsert_relayed_message().times(0);
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_send_text().times(0);

    let relay = relay_use_case(repo, gateway, text_sync_service(0));
    relay.relay(&message(10, "")).await.unwrap();
}
