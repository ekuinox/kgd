//! 書き込み用チャンネル投稿の転記処理のテスト。

use mockall::predicate::eq;

use crate::test_support::{entry, text_sync_service, utc};

use super::*;

/// 書き込み用チャンネルの投稿を relay した場合、今日の日報スレッドへ本文と元投稿リンクを付けて
/// 転記し、転記マッピングを記録し、元投稿に ✅ リアクションを付けることを確認する。
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

/// 今日の日報が存在しない場合、過去の日報へ転記せず、書き込み用チャンネルへ
/// 案内メッセージだけを送ることを確認する。
#[tokio::test]
async fn relay_notifies_instead_of_relaying_to_a_past_diary() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(1).returning(|_| Ok(None));
    repo.expect_get_latest_entry().times(0);
    repo.expect_upsert_relayed_message().times(0);
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_send_text()
        .withf(|channel_id, content| {
            *channel_id == 500
                && content.contains("日報作成ボタン")
                && content.contains("投稿し直して")
        })
        .times(1)
        .returning(|_, _| Ok(901));

    let relay = relay_use_case(repo, gateway, text_sync_service(0));
    relay.relay(&message(10, "hello")).await.unwrap();
}

/// 今日の日報が無い状態で複数回投稿されても、案内メッセージは同じ日に一度しか
/// 送らない (times(1)) ことを確認する。
#[tokio::test]
async fn relay_notifies_missing_diary_only_once_per_day() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(2).returning(|_| Ok(None));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_send_text()
        .times(1)
        .returning(|_, _| Ok(901));

    let relay = relay_use_case(repo, gateway, text_sync_service(0));
    relay.relay(&message(10, "hello")).await.unwrap();
    relay.relay(&message(11, "world")).await.unwrap();
}

/// 案内の送信に失敗した場合は通知済みとして記録せず、次の投稿で送り直すことを確認する。
///
/// 失敗したまま記録すると、その日はもう案内が届かなくなるため。
#[tokio::test]
async fn relay_retries_the_notice_when_sending_it_failed() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date().times(2).returning(|_| Ok(None));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_send_text()
        .times(2)
        .returning(|_, _| Err(anyhow::anyhow!("failed to send")));

    let relay = relay_use_case(repo, gateway, text_sync_service(0));

    // 案内の失敗は転記処理の失敗としては扱わない
    relay.relay(&message(10, "hello")).await.unwrap();
    relay.relay(&message(11, "world")).await.unwrap();
}

/// 日報スレッドはあるが本文が空のメッセージを relay した場合、転記投稿(send_text)も
/// マッピング記録(upsert_relayed_message)も行わない(times(0))ことを確認する。
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
