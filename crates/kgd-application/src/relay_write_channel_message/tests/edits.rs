//! 転記済みメッセージの編集・削除のテスト。

use mockall::predicate::eq;

use kgd_domain::compile_url_rules;

use crate::ports::{MockAttachmentDownloader, MockImageConverter, MockNotionApi};
use crate::test_support::{entry, text_sync_service, utc};

use super::*;

#[tokio::test]
async fn update_edits_relayed_message() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_relayed_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| {
            Ok(Some(RelayedMessage {
                source_message_id: 10,
                thread_id: 100,
                relayed_message_id: 900,
            }))
        });
    repo.expect_get_by_thread()
        .with(eq(100u64))
        .times(1)
        .returning(|_| Ok(Some(entry(100, utc(2025, 1, 2, 0, 0)))));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_edit_message_content()
        .withf(|thread_id, message_id, content| {
            *thread_id == 100 && *message_id == 900 && content.contains("edited")
        })
        .times(1)
        .returning(|_, _, _| Ok(()));

    // 旧ブロック削除 + 再同期を行う SyncDiaryMessage
    let url_rules =
        compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
    let mut notion = MockNotionApi::new();
    notion.expect_delete_block().times(1).returning(|_| Ok(()));
    notion
        .expect_append_blocks()
        .times(1)
        .returning(|_, _| Ok(vec!["b2".to_string()]));
    let mut sync_repo = MockDiaryRepository::new();
    sync_repo
        .expect_get_blocks_by_message()
        .times(1)
        .returning(|_| {
            Ok(vec![kgd_domain::MessageBlock {
                message_id: 10,
                block_id: "b1".to_string(),
                block_type: "text".to_string(),
                block_order: 0,
            }])
        });
    sync_repo
        .expect_delete_blocks_by_message()
        .times(1)
        .returning(|_| Ok(()));
    sync_repo
        .expect_insert_message_block()
        .times(1)
        .returning(|_| Ok(()));
    let sync = Arc::new(SyncDiaryMessage::new(
        Arc::new(notion),
        Arc::new(sync_repo),
        Arc::new(MockAttachmentDownloader::new()),
        Arc::new(MockImageConverter::new()),
        None,
        url_rules,
    ));

    let relay = relay_use_case(repo, gateway, sync);
    relay.update(&message(10, "edited")).await.unwrap();
}

#[tokio::test]
async fn update_ignores_messages_not_relayed() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_relayed_message()
        .times(1)
        .returning(|_| Ok(None));
    let mut gateway = MockDiscordGateway::new();
    gateway.expect_edit_message_content().times(0);

    let relay = relay_use_case(repo, gateway, text_sync_service(0));
    relay.update(&message(10, "edited")).await.unwrap();
}

#[tokio::test]
async fn delete_removes_blocks_relayed_message_and_mapping() {
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_relayed_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| {
            Ok(Some(RelayedMessage {
                source_message_id: 10,
                thread_id: 100,
                relayed_message_id: 900,
            }))
        });
    repo.expect_delete_relayed_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| Ok(()));
    let mut gateway = MockDiscordGateway::new();
    gateway
        .expect_delete_message()
        .with(eq(100u64), eq(900u64))
        .times(1)
        .returning(|_, _| Ok(()));

    // Notion 側のブロック削除
    let url_rules =
        compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
    let mut notion = MockNotionApi::new();
    notion.expect_delete_block().times(1).returning(|_| Ok(()));
    let mut sync_repo = MockDiaryRepository::new();
    sync_repo
        .expect_get_blocks_by_message()
        .times(1)
        .returning(|_| {
            Ok(vec![kgd_domain::MessageBlock {
                message_id: 10,
                block_id: "b1".to_string(),
                block_type: "text".to_string(),
                block_order: 0,
            }])
        });
    sync_repo
        .expect_delete_blocks_by_message()
        .times(1)
        .returning(|_| Ok(()));
    let sync = Arc::new(SyncDiaryMessage::new(
        Arc::new(notion),
        Arc::new(sync_repo),
        Arc::new(MockAttachmentDownloader::new()),
        Arc::new(MockImageConverter::new()),
        None,
        url_rules,
    ));

    let relay = relay_use_case(repo, gateway, sync);
    relay.delete(10).await.unwrap();
}
