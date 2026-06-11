//! メッセージ更新・削除に伴うブロック操作のテスト。

use mockall::predicate::eq;

use super::*;

#[tokio::test]
async fn update_returns_false_when_no_blocks() {
    let mut builder = TestSyncBuilder::new();
    builder
        .repo
        .expect_get_blocks_by_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| Ok(vec![]));
    builder.notion.expect_update_text_block().times(0);

    let sync = builder.build();
    let updated = sync.update(&text_message(10, "edited")).await.unwrap();

    assert!(!updated);
}

#[tokio::test]
async fn update_updates_text_blocks_only() {
    let mut builder = TestSyncBuilder::new();
    builder
        .repo
        .expect_get_blocks_by_message()
        .times(1)
        .returning(|_| {
            Ok(vec![
                MessageBlock {
                    message_id: 10,
                    block_id: "img-block".to_string(),
                    block_type: "image".to_string(),
                    block_order: 0,
                },
                MessageBlock {
                    message_id: 10,
                    block_id: "text-block".to_string(),
                    block_type: "text".to_string(),
                    block_order: 1,
                },
            ])
        });
    // テキストブロックのみ更新される
    builder
        .notion
        .expect_update_text_block()
        .withf(|block_id, _| block_id == "text-block")
        .times(1)
        .returning(|_, _| Ok(()));

    let sync = builder.build();
    let updated = sync.update(&text_message(10, "edited")).await.unwrap();

    assert!(updated);
}

#[tokio::test]
async fn delete_removes_all_blocks_and_db_rows() {
    let mut builder = TestSyncBuilder::new();
    builder
        .repo
        .expect_get_blocks_by_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| {
            Ok(vec![
                MessageBlock {
                    message_id: 10,
                    block_id: "b1".to_string(),
                    block_type: "text".to_string(),
                    block_order: 0,
                },
                MessageBlock {
                    message_id: 10,
                    block_id: "b2".to_string(),
                    block_type: "image".to_string(),
                    block_order: 1,
                },
            ])
        });
    builder
        .notion
        .expect_delete_block()
        .times(2)
        .returning(|_| Ok(()));
    builder
        .repo
        .expect_delete_blocks_by_message()
        .with(eq(10u64))
        .times(1)
        .returning(|_| Ok(()));

    let sync = builder.build();
    let deleted = sync.delete(10).await.unwrap();

    assert!(deleted);
}

#[tokio::test]
async fn delete_returns_false_when_no_blocks() {
    let mut builder = TestSyncBuilder::new();
    builder
        .repo
        .expect_get_blocks_by_message()
        .times(1)
        .returning(|_| Ok(vec![]));
    builder.notion.expect_delete_block().times(0);
    builder.repo.expect_delete_blocks_by_message().times(0);

    let sync = builder.build();
    let deleted = sync.delete(10).await.unwrap();

    assert!(!deleted);
}
