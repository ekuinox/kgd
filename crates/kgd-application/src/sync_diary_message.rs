//! Discord メッセージを Notion に同期するユースケース。

use std::{collections::HashMap, sync::Arc};

use anyhow::{Context as _, Result};
use tracing::warn;

use kgd_domain::{
    CompiledUrlRules, FileType, MessageBlock, OgpMetadata, SyncAttachment, SyncMessage,
    apply_ogp_to_bookmark, build_rich_text_and_url_blocks, classify_file, file_block_json,
    image_block_json, is_spoiler_attachment, replace_extension, spoiler_summary, toggle_block_json,
};

use super::ports::{AttachmentDownloader, DiaryRepository, ImageConverter, NotionApi, OgpClient};

/// 同期結果の情報。
pub struct SyncResult {
    /// 同期が実行されたかどうか
    pub synced: bool,
    /// 作成されたブロック数
    pub block_count: usize,
}

/// メッセージを Notion に同期するユースケース。
///
/// 外部 IO はすべてポート経由で行うため、mockall のモックで単体テストできる。
pub struct SyncDiaryMessage {
    /// Notion API ポート
    notion: Arc<dyn NotionApi>,
    /// 日報リポジトリポート
    repo: Arc<dyn DiaryRepository>,
    /// 添付ファイルダウンローダーポート
    downloader: Arc<dyn AttachmentDownloader>,
    /// 画像変換ポート
    converter: Arc<dyn ImageConverter>,
    /// OGP クライアントポート（None の場合は OGP 取得を行わない）
    ogp: Option<Arc<dyn OgpClient>>,
    /// URL 変換ルール（コンパイル済み）
    url_rules: CompiledUrlRules,
}

impl SyncDiaryMessage {
    /// 新しい SyncDiaryMessage を作成する。
    pub fn new(
        notion: Arc<dyn NotionApi>,
        repo: Arc<dyn DiaryRepository>,
        downloader: Arc<dyn AttachmentDownloader>,
        converter: Arc<dyn ImageConverter>,
        ogp: Option<Arc<dyn OgpClient>>,
        url_rules: CompiledUrlRules,
    ) -> Self {
        Self {
            notion,
            repo,
            downloader,
            converter,
            ogp,
            url_rules,
        }
    }

    /// メッセージを Notion ページに同期する。
    ///
    /// テキストと添付ファイルのブロックを1回の API 呼び出しでまとめて追加することで、
    /// ブロック間に不要な空行が入るのを防ぐ。
    ///
    /// # Returns
    /// 同期結果（同期されたかどうかと作成されたブロック数）
    pub async fn sync(&self, page_id: &str, message: &SyncMessage) -> Result<SyncResult> {
        let has_content = !message.content.is_empty();
        let has_attachments = !message.attachments.is_empty();

        if !has_content && !has_attachments {
            return Ok(SyncResult {
                synced: false,
                block_count: 0,
            });
        }

        // ブロック JSON とメタ情報（block_type）を収集する
        // 順序: 添付ファイル（画像埋め込み → ファイルリンク） → テキスト
        let mut children: Vec<serde_json::Value> = Vec::new();
        let mut block_meta: Vec<String> = Vec::new(); // 各ブロックの種別

        // 添付ファイル: ファイルをアップロードしてブロック JSON を収集
        for attachment in &message.attachments {
            self.prepare_attachment_blocks(attachment, &mut children, &mut block_meta)
                .await?;
        }

        // テキストブロック（URL をリンク化 + ルールに基づく追加ブロック生成）
        // 出現順に paragraph / bookmark / embed ブロックが並ぶ
        if has_content {
            let result = build_rich_text_and_url_blocks(&message.content, &self.url_rules);

            // OGP メタデータを並列取得
            let ogp_map = self.fetch_ogp_for_bookmarks(&result.bookmark_urls).await;

            for (mut block_json, block_type) in result.blocks {
                // ブックマークブロックに OGP メタデータを適用
                if block_type == "bookmark"
                    && let Some(url) = block_json["bookmark"]["url"].as_str()
                    && let Some(ogp) = ogp_map.get(url)
                {
                    apply_ogp_to_bookmark(&mut block_json, ogp);
                }
                children.push(block_json);
                block_meta.push(block_type);
            }
        }

        if children.is_empty() {
            return Ok(SyncResult {
                synced: false,
                block_count: 0,
            });
        }

        // 全ブロックを一括で追加
        let block_ids = self.notion.append_blocks(page_id, children).await?;

        // DB にブロック情報を保存
        for (i, (block_id, block_type)) in block_ids.into_iter().zip(block_meta.iter()).enumerate()
        {
            let message_block = MessageBlock {
                message_id: message.message_id,
                block_id,
                block_type: block_type.to_string(),
                block_order: i as i32,
            };
            self.repo.insert_message_block(&message_block).await?;
        }

        Ok(SyncResult {
            synced: true,
            block_count: block_meta.len(),
        })
    }

    /// メッセージが更新されたときに Notion ブロックを更新する。
    ///
    /// テキストブロックのみ更新可能。画像・ブックマークブロックは更新されない。
    pub async fn update(&self, message: &SyncMessage) -> Result<bool> {
        let blocks = self.repo.get_blocks_by_message(message.message_id).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // テキストブロックのみ更新（URL をリンク化）
        let result = build_rich_text_and_url_blocks(&message.content, &self.url_rules);
        let text_rich_texts: Vec<Vec<serde_json::Value>> = result
            .blocks
            .iter()
            .filter(|(_, block_type)| block_type == "text")
            .filter_map(|(block_json, _)| block_json["paragraph"]["rich_text"].as_array().cloned())
            .collect();

        let text_blocks: Vec<&MessageBlock> =
            blocks.iter().filter(|b| b.block_type == "text").collect();

        for (block, rich_text) in text_blocks.iter().zip(text_rich_texts.iter()) {
            self.notion
                .update_text_block(&block.block_id, rich_text.clone())
                .await?;
        }

        Ok(true)
    }

    /// メッセージが削除されたときに対応する Notion ブロックを削除する。
    pub async fn delete(&self, message_id: u64) -> Result<bool> {
        let blocks = self.repo.get_blocks_by_message(message_id).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // すべてのブロックを削除
        for block in &blocks {
            self.notion.delete_block(&block.block_id).await?;
        }

        // DB からブロック情報を削除
        self.repo.delete_blocks_by_message(message_id).await?;

        Ok(true)
    }

    /// 添付ファイルをアップロードし、対応するブロック JSON とメタ情報を収集する。
    ///
    /// HEIC の場合は JPG 変換版（画像ブロック）と元ファイル（ファイルブロック）の 2 つを追加する。
    async fn prepare_attachment_blocks(
        &self,
        attachment: &SyncAttachment,
        children: &mut Vec<serde_json::Value>,
        block_meta: &mut Vec<String>,
    ) -> Result<()> {
        let file_type = classify_file(&attachment.filename);
        let mut attachment_children = Vec::new();
        let mut attachment_block_meta = Vec::new();

        match file_type {
            FileType::Image => {
                let (data, content_type) = self.downloader.download(attachment).await?;
                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .context("Failed to upload image to Notion")?;
                attachment_children.push(image_block_json(&file_upload_id));
                attachment_block_meta.push("image".to_string());
            }
            FileType::Heic => {
                let (data, content_type) = self.downloader.download(attachment).await?;

                // HEIC を JPEG に変換してアップロード（変換不可ならスキップ）
                match self.converter.heic_to_jpeg(&data) {
                    Ok(jpeg_data) => {
                        let jpeg_filename = replace_extension(&attachment.filename, "jpg");
                        let jpeg_upload_id = self
                            .notion
                            .upload_file(&jpeg_filename, "image/jpeg", jpeg_data)
                            .await
                            .context("Failed to upload converted JPEG to Notion")?;
                        attachment_children.push(image_block_json(&jpeg_upload_id));
                        attachment_block_meta.push("image".to_string());
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to convert HEIC to JPEG, skipping conversion");
                    }
                }

                // 元の HEIC ファイルもアップロード
                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to upload file to Notion: filename={}, content_type={}",
                            attachment.filename, content_type
                        )
                    })?;
                attachment_children.push(file_block_json(&file_upload_id, &attachment.filename));
                attachment_block_meta.push("file".to_string());
            }
            FileType::Other => {
                let (data, content_type) = self.downloader.download(attachment).await?;

                tracing::debug!(
                    filename = %attachment.filename,
                    content_type = %content_type,
                    size = data.len(),
                    "Uploading file to Notion"
                );

                let file_upload_id = self
                    .notion
                    .upload_file(&attachment.filename, &content_type, data)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to upload file to Notion: filename={}, content_type={}",
                            attachment.filename, content_type
                        )
                    })?;
                attachment_children.push(file_block_json(&file_upload_id, &attachment.filename));
                attachment_block_meta.push("file".to_string());
            }
        }

        if matches!(file_type, FileType::Image | FileType::Heic)
            && is_spoiler_attachment(&attachment.filename)
        {
            let summary = spoiler_summary(attachment.description.as_deref());
            children.push(toggle_block_json(&summary, attachment_children));
            block_meta.push("toggle".to_string());
        } else {
            children.extend(attachment_children);
            block_meta.extend(attachment_block_meta);
        }

        Ok(())
    }

    /// Bookmark URL の OGP メタデータを並列で取得する。
    async fn fetch_ogp_for_bookmarks(&self, urls: &[String]) -> HashMap<String, OgpMetadata> {
        let Some(ogp) = &self.ogp else {
            return HashMap::new();
        };

        if urls.is_empty() {
            return HashMap::new();
        }

        ogp.fetch_many(urls).await
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use crate::ports::{
        MockAttachmentDownloader, MockDiaryRepository, MockImageConverter, MockNotionApi,
        MockOgpClient,
    };
    use kgd_domain::compile_url_rules;

    use super::*;

    /// テスト用のユースケースを構築するビルダー。
    struct TestSyncBuilder {
        notion: MockNotionApi,
        repo: MockDiaryRepository,
        downloader: MockAttachmentDownloader,
        converter: MockImageConverter,
        ogp: Option<MockOgpClient>,
    }

    impl TestSyncBuilder {
        fn new() -> Self {
            Self {
                notion: MockNotionApi::new(),
                repo: MockDiaryRepository::new(),
                downloader: MockAttachmentDownloader::new(),
                converter: MockImageConverter::new(),
                ogp: None,
            }
        }

        fn build(self) -> SyncDiaryMessage {
            let url_rules =
                compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
            SyncDiaryMessage::new(
                Arc::new(self.notion),
                Arc::new(self.repo),
                Arc::new(self.downloader),
                Arc::new(self.converter),
                self.ogp.map(|ogp| Arc::new(ogp) as Arc<dyn OgpClient>),
                url_rules,
            )
        }
    }

    /// テスト用のテキストメッセージを作る。
    fn text_message(message_id: u64, content: &str) -> SyncMessage {
        SyncMessage {
            message_id,
            channel_id: 1,
            content: content.to_string(),
            is_bot: false,
            attachments: vec![],
        }
    }

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
                children.len() == 2
                    && children[0]["type"] == "image"
                    && children[1]["type"] == "file"
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
            SyncDiaryMessage::new(
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
}
