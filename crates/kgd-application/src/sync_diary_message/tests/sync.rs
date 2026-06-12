//! テキスト同期と OGP 適用のテスト。

use super::*;

/// 空メッセージを sync した場合、Notion への append も DB への挿入も行わず(times(0))、
/// synced=false・block_count=0 を返すことを確認する。
#[tokio::test]
async fn sync_skips_empty_message() {
    let mut builder = TestSyncBuilder::new();
    // 空メッセージでは Notion / DB を一切呼ばない
    builder.notion.expect_append_blocks().times(0);
    builder.repo.expect_insert_message_block().times(0);

    let sync = builder.build();
    let result = sync.sync("page-1", &text_message(10, "")).await.unwrap();

    assert!(!result.synced);
    assert_eq!(result.block_count, 0);
}

/// テキストメッセージを sync した場合、paragraph ブロックを 1 件 append し、
/// 返ってきた block_id を text タイプ・order 0 で DB に記録することを確認する。
#[tokio::test]
async fn sync_appends_text_block_and_stores_mapping() {
    let mut builder = TestSyncBuilder::new();
    builder
        .notion
        .expect_append_blocks()
        .withf(|page_id, children| {
            page_id == "page-1" && children.len() == 1 && children[0]["type"] == "paragraph"
        })
        .times(1)
        .returning(|_, _| Ok(vec!["block-1".to_string()]));
    builder
        .repo
        .expect_insert_message_block()
        .withf(|block| {
            block.message_id == 10
                && block.block_id == "block-1"
                && block.block_type == "text"
                && block.block_order == 0
        })
        .times(1)
        .returning(|_| Ok(()));

    let sync = builder.build();
    let result = sync
        .sync("page-1", &text_message(10, "hello world"))
        .await
        .unwrap();

    assert!(result.synced);
    assert_eq!(result.block_count, 1);
}

/// URL を bookmark 化するルール下で sync した場合、OGP メタデータを fetch_many で取得し、
/// 取得したタイトルを bookmark ブロックの caption に反映することを確認する。
#[tokio::test]
async fn sync_applies_ogp_to_bookmark_blocks() {
    let mut builder = TestSyncBuilder::new();
    let mut ogp = MockOgpClient::new();
    ogp.expect_fetch_many()
        .withf(|urls| urls.len() == 1 && urls[0] == "https://example.com/article")
        .times(1)
        .returning(|_| {
            let mut map = HashMap::new();
            map.insert(
                "https://example.com/article".to_string(),
                OgpMetadata {
                    title: Some("Example Article".to_string()),
                    description: None,
                },
            );
            map
        });
    builder.ogp = Some(ogp);

    builder
        .notion
        .expect_append_blocks()
        .withf(|_, children| {
            children.iter().any(|block| {
                block["type"] == "bookmark"
                    && block["bookmark"]["caption"][0]["text"]["content"] == "Example Article"
            })
        })
        .times(1)
        .returning(|_, children| Ok((0..children.len()).map(|i| format!("b{}", i)).collect()));
    builder
        .repo
        .expect_insert_message_block()
        .returning(|_| Ok(()));

    let sync = {
        // bookmark に変換するルールで構築する
        let url_rules =
            compile_url_rules(&[], &["bookmark".to_string()]).expect("rules should compile");
        SyncDiaryMessageUseCase::new(
            Arc::new(builder.notion),
            Arc::new(builder.repo),
            Arc::new(builder.downloader),
            Arc::new(builder.converter),
            builder.ogp.map(|ogp| Arc::new(ogp) as Arc<dyn OgpClient>),
            url_rules,
        )
    };

    let result = sync
        .sync("page-1", &text_message(10, "https://example.com/article"))
        .await
        .unwrap();

    assert!(result.synced);
}
