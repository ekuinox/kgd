//! 添付ファイル同期（順序・スポイラー）のテスト。

use mockall::predicate::eq;

use kgd_domain::SyncAttachment;

use super::*;

/// 画像添付とテキストを持つメッセージを sync した場合、image ブロックを
/// paragraph(テキスト)より前に配置し、2 ブロックを記録することを確認する。
#[tokio::test]
async fn sync_places_attachments_before_text() {
    let mut builder = TestSyncBuilder::new();
    builder
        .downloader
        .expect_download()
        .times(1)
        .returning(|_| Ok((vec![1, 2, 3], "image/png".to_string())));
    builder
        .notion
        .expect_upload_file()
        .with(eq("photo.png"), eq("image/png"), eq(vec![1, 2, 3]))
        .times(1)
        .returning(|_, _, _| Ok("upload-1".to_string()));
    builder
        .notion
        .expect_append_blocks()
        .withf(|_, children| {
            // 添付ファイル（画像） → テキストの順
            children.len() == 2
                && children[0]["type"] == "image"
                && children[1]["type"] == "paragraph"
        })
        .times(1)
        .returning(|_, _| Ok(vec!["block-1".to_string(), "block-2".to_string()]));
    builder
        .repo
        .expect_insert_message_block()
        .times(2)
        .returning(|_| Ok(()));

    let message = SyncMessage {
        message_id: 10,
        channel_id: 1,
        guild_id: Some(1),
        content: "caption".to_string(),
        is_bot: false,
        attachments: vec![SyncAttachment {
            filename: "photo.png".to_string(),
            url: "https://cdn.example/photo.png".to_string(),
            description: None,
        }],
    };

    let sync = TestSyncBuilder::build(builder);
    let result = sync.sync("page-1", &message).await.unwrap();

    assert!(result.synced);
    assert_eq!(result.block_count, 2);
}

/// スポイラー(SPOILER_ 接頭辞)の画像添付を sync した場合、画像を toggle ブロックで包み、
/// toggle タイプとして 1 ブロック記録することを確認する。
#[tokio::test]
async fn sync_wraps_spoiler_image_in_toggle() {
    let mut builder = TestSyncBuilder::new();
    builder
        .downloader
        .expect_download()
        .times(1)
        .returning(|_| Ok((vec![1], "image/png".to_string())));
    builder
        .notion
        .expect_upload_file()
        .times(1)
        .returning(|_, _, _| Ok("upload-1".to_string()));
    builder
        .notion
        .expect_append_blocks()
        .withf(|_, children| children.len() == 1 && children[0]["type"] == "toggle")
        .times(1)
        .returning(|_, _| Ok(vec!["block-1".to_string()]));
    builder
        .repo
        .expect_insert_message_block()
        .withf(|block| block.block_type == "toggle")
        .times(1)
        .returning(|_| Ok(()));

    let message = SyncMessage {
        message_id: 10,
        channel_id: 1,
        guild_id: Some(1),
        content: String::new(),
        is_bot: false,
        attachments: vec![SyncAttachment {
            filename: "SPOILER_photo.png".to_string(),
            url: "https://cdn.example/SPOILER_photo.png".to_string(),
            description: Some("hidden".to_string()),
        }],
    };

    let sync = TestSyncBuilder::build(builder);
    let result = sync.sync("page-1", &message).await.unwrap();

    assert!(result.synced);
    assert_eq!(result.block_count, 1);
}
