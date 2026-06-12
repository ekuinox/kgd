//! HEIC 添付ファイルの変換・アップロードのテスト。

use mockall::predicate::eq;

use kgd_domain::SyncAttachment;

use super::*;

/// HEIC 添付を sync した場合、JPEG へ変換して image ブロックを、元 HEIC を file ブロックとして
/// この順でアップロード・追加し、2 ブロックを記録することを確認する。
#[tokio::test]
async fn sync_uploads_heic_as_jpeg_and_original_file() {
    let mut builder = TestSyncBuilder::new();
    builder
        .downloader
        .expect_download()
        .times(1)
        .returning(|_| Ok((vec![9, 9], "image/heic".to_string())));
    builder
        .converter
        .expect_heic_to_jpeg()
        .times(1)
        .returning(|_| Ok(vec![7, 7]));
    // JPEG 変換版 → 元ファイルの順でアップロードされる
    builder
        .notion
        .expect_upload_file()
        .with(eq("photo.jpg"), eq("image/jpeg"), eq(vec![7, 7]))
        .times(1)
        .returning(|_, _, _| Ok("upload-jpeg".to_string()));
    builder
        .notion
        .expect_upload_file()
        .with(eq("photo.heic"), eq("image/heic"), eq(vec![9, 9]))
        .times(1)
        .returning(|_, _, _| Ok("upload-heic".to_string()));
    builder
        .notion
        .expect_append_blocks()
        .withf(|_, children| {
            children.len() == 2 && children[0]["type"] == "image" && children[1]["type"] == "file"
        })
        .times(1)
        .returning(|_, _| Ok(vec!["b1".to_string(), "b2".to_string()]));
    builder
        .repo
        .expect_insert_message_block()
        .times(2)
        .returning(|_| Ok(()));

    let message = SyncMessage {
        message_id: 10,
        channel_id: 1,
        guild_id: Some(1),
        content: String::new(),
        is_bot: false,
        attachments: vec![SyncAttachment {
            filename: "photo.heic".to_string(),
            url: "https://cdn.example/photo.heic".to_string(),
            description: None,
        }],
    };

    let sync = TestSyncBuilder::build(builder);
    let result = sync.sync("page-1", &message).await.unwrap();

    assert!(result.synced);
    assert_eq!(result.block_count, 2);
}

/// HEIC の JPEG 変換が失敗した場合、image ブロックを作らず元 HEIC を file ブロックとして
/// のみアップロードし、1 ブロックを記録することを確認する。
#[tokio::test]
async fn sync_skips_jpeg_block_when_conversion_fails() {
    let mut builder = TestSyncBuilder::new();
    builder
        .downloader
        .expect_download()
        .times(1)
        .returning(|_| Ok((vec![9, 9], "image/heic".to_string())));
    builder
        .converter
        .expect_heic_to_jpeg()
        .times(1)
        .returning(|_| anyhow::bail!("not supported"));
    // 変換失敗時は元ファイルのみアップロードされる
    builder
        .notion
        .expect_upload_file()
        .with(eq("photo.heic"), eq("image/heic"), eq(vec![9, 9]))
        .times(1)
        .returning(|_, _, _| Ok("upload-heic".to_string()));
    builder
        .notion
        .expect_append_blocks()
        .withf(|_, children| children.len() == 1 && children[0]["type"] == "file")
        .times(1)
        .returning(|_, _| Ok(vec!["b1".to_string()]));
    builder
        .repo
        .expect_insert_message_block()
        .times(1)
        .returning(|_| Ok(()));

    let message = SyncMessage {
        message_id: 10,
        channel_id: 1,
        guild_id: Some(1),
        content: String::new(),
        is_bot: false,
        attachments: vec![SyncAttachment {
            filename: "photo.heic".to_string(),
            url: "https://cdn.example/photo.heic".to_string(),
            description: None,
        }],
    };

    let sync = TestSyncBuilder::build(builder);
    let result = sync.sync("page-1", &message).await.unwrap();

    assert!(result.synced);
    assert_eq!(result.block_count, 1);
}
