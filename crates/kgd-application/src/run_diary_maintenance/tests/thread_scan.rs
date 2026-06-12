//! スレッド走査と同期リアクションのテスト。

use mockall::Sequence;
use mockall::predicate::eq;

use kgd_domain::ThreadState;

use crate::test_support::{empty_sync_service, entry, fixed_clock, text_sync_service, utc};

use super::*;

/// スレッドのメッセージをページングで走査し、bot メッセージは除外、同期済みは already_synced、
/// 未同期でも空コンテンツのものは skipped として集計し、件数(checked=2, already=1, synced=0, skipped=1)
/// が正しいことを確認する。
#[tokio::test]
async fn sync_missing_pages_through_thread_and_counts() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(Some(entry(100, utc(2025, 1, 1, 0, 0)))));
    // メッセージ 1 は同期済み、メッセージ 2 は未同期（空コンテンツでスキップされる）
    repo.expect_has_blocks_by_message()
        .with(eq(1u64))
        .times(1)
        .returning(|_| Ok(true));
    repo.expect_has_blocks_by_message()
        .with(eq(2u64))
        .times(1)
        .returning(|_| Ok(false));

    let mut gateway = MockDiscordGateway::new();
    gateway.expect_thread_state().times(1).returning(|_| {
        Ok(Some(ThreadState {
            is_public_thread: true,
            archived: false,
            locked: false,
        }))
    });

    // ページング: 1 回目は 3 件（うち 1 件は bot）、2 回目は空
    let mut seq = Sequence::new();
    gateway
        .expect_fetch_messages_before()
        .with(eq(100u64), eq(None), eq(THREAD_SYNC_BATCH_SIZE))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _, _| {
            Ok(vec![
                SyncMessage {
                    message_id: 2,
                    channel_id: 100,
                    guild_id: Some(1),
                    content: String::new(),
                    is_bot: false,
                    attachments: vec![],
                },
                SyncMessage {
                    message_id: 9,
                    channel_id: 100,
                    guild_id: Some(1),
                    content: "bot message".to_string(),
                    is_bot: true,
                    attachments: vec![],
                },
                SyncMessage {
                    message_id: 1,
                    channel_id: 100,
                    guild_id: Some(1),
                    content: "synced".to_string(),
                    is_bot: false,
                    attachments: vec![],
                },
            ])
        });
    gateway
        .expect_fetch_messages_before()
        .with(eq(100u64), eq(Some(1u64)), eq(THREAD_SYNC_BATCH_SIZE))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _, _| Ok(vec![]));

    let clock = fixed_clock(utc(2025, 1, 1, 12, 0));

    let m = maintenance(repo, gateway, clock, empty_sync_service());
    let report = m.sync_missing_in_thread(100).await.unwrap();

    // bot を除く 2 件を確認し、1 件は同期済み・1 件は空メッセージでスキップ
    assert_eq!(report.checked_messages, 2);
    assert_eq!(report.already_synced_messages, 1);
    assert_eq!(report.synced_messages, 0);
    assert_eq!(report.skipped_messages, 1);
}

/// sync_message_with_reaction で同期が成功した場合、対象メッセージへ ✅ リアクションを付け、
/// synced=true・blocks=1 を返すことを確認する。
#[tokio::test]
async fn sync_message_with_reaction_adds_reaction_on_success() {
    let sync = text_sync_service(1);

    let repo = MockDiaryRepository::new();
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_add_reaction()
        .with(eq(100u64), eq(10u64), eq("✅"))
        .times(1)
        .returning(|_, _, _| Ok(()));
    let clock = fixed_clock(utc(2025, 1, 1, 12, 0));

    let m = maintenance(repo, gateway, clock, sync);
    let message = SyncMessage {
        message_id: 10,
        channel_id: 100,
        guild_id: Some(1),
        content: "hello".to_string(),
        is_bot: false,
        attachments: vec![],
    };
    let (synced, blocks) = m
        .sync_message_with_reaction("page-100", &message)
        .await
        .unwrap();

    assert!(synced);
    assert_eq!(blocks, 1);
}
