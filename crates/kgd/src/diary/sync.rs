//! Discord メッセージを Notion に同期する機能を提供する。

use anyhow::{Context as _, Result};
use serenity::model::channel::{Attachment, Message};

use super::{DiaryStore, MessageBlock, NotionClient};

/// 同期結果の情報。
pub struct SyncResult {
    /// 同期が実行されたかどうか
    pub synced: bool,
    /// 作成されたブロック数
    pub block_count: usize,
}

/// メッセージを Notion に同期するためのシンクロナイザー。
pub struct MessageSyncer<'a> {
    /// Notion クライアント
    notion: &'a NotionClient,
    /// 日報ストア
    store: &'a DiaryStore,
    /// HTTP クライアント（画像ダウンロード用）
    http_client: reqwest::Client,
}

impl<'a> MessageSyncer<'a> {
    /// 新しい MessageSyncer を作成する。
    pub fn new(notion: &'a NotionClient, store: &'a DiaryStore) -> Self {
        Self {
            notion,
            store,
            http_client: reqwest::Client::new(),
        }
    }

    /// メッセージを Notion ページに同期する。
    ///
    /// # Returns
    /// 同期結果（同期されたかどうかと作成されたブロック情報）
    pub async fn sync_message(&self, page_id: &str, message: &Message) -> Result<SyncResult> {
        let has_content = !message.content.is_empty();
        let has_attachments = !message.attachments.is_empty();

        if !has_content && !has_attachments {
            return Ok(SyncResult {
                synced: false,
                block_count: 0,
            });
        }

        let mut block_count = 0;
        let mut block_order = 0;

        // テキストコンテンツを同期
        if has_content {
            let block_id = self
                .notion
                .append_text_block_with_id(page_id, &message.content)
                .await?;

            // DB にブロック情報を保存
            let message_block = MessageBlock {
                message_id: message.id.get(),
                block_id,
                block_type: "text".to_string(),
                block_order,
            };
            self.store.insert_message_block(&message_block).await?;

            block_count += 1;
            block_order += 1;
        }

        // 添付ファイルを同期
        for attachment in &message.attachments {
            self.sync_attachment_with_tracking(page_id, message.id.get(), attachment, block_order)
                .await?;
            block_count += 1;
            block_order += 1;
        }

        Ok(SyncResult {
            synced: true,
            block_count,
        })
    }

    /// メッセージが更新されたときに Notion ブロックを更新する。
    ///
    /// テキストブロックのみ更新可能。画像ブロックは更新されない。
    pub async fn update_message(&self, message: &Message) -> Result<bool> {
        let blocks = self.store.get_blocks_by_message(message.id.get()).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // テキストブロックのみ更新
        for block in blocks.iter().filter(|b| b.block_type == "text") {
            self.notion
                .update_text_block(&block.block_id, &message.content)
                .await?;
        }

        Ok(true)
    }

    /// メッセージが削除されたときに対応する Notion ブロックを削除する。
    pub async fn delete_message(&self, message_id: u64) -> Result<bool> {
        let blocks = self.store.get_blocks_by_message(message_id).await?;

        if blocks.is_empty() {
            return Ok(false);
        }

        // すべてのブロックを削除
        for block in &blocks {
            self.notion.delete_block(&block.block_id).await?;
        }

        // DB からブロック情報を削除
        self.store.delete_blocks_by_message(message_id).await?;

        Ok(true)
    }

    /// 添付ファイルを Notion に同期し、ブロック情報を追跡する。
    async fn sync_attachment_with_tracking(
        &self,
        page_id: &str,
        message_id: u64,
        attachment: &Attachment,
        block_order: i32,
    ) -> Result<()> {
        // 画像の場合はダウンロードしてNotionにアップロード
        let (block_id, block_type) = if is_image(&attachment.filename) {
            let id = self.upload_image_with_id(page_id, attachment).await?;
            (id, "image")
        } else {
            // その他のファイルはリンクとしてテキストブロックに追加
            let text = format!("[{}]({})", attachment.filename, attachment.url);
            let id = self
                .notion
                .append_text_block_with_id(page_id, &text)
                .await?;
            (id, "link")
        };

        // DB にブロック情報を保存
        let message_block = MessageBlock {
            message_id,
            block_id,
            block_type: block_type.to_string(),
            block_order,
        };
        self.store.insert_message_block(&message_block).await?;

        Ok(())
    }

    /// 画像をダウンロードしてNotionにアップロードし、ブロック ID を返す。
    async fn upload_image_with_id(&self, page_id: &str, attachment: &Attachment) -> Result<String> {
        // Discord から画像をダウンロード
        let response = self
            .http_client
            .get(&attachment.url)
            .send()
            .await
            .context("Failed to download image from Discord")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download image: status = {}", response.status());
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();

        let data = response
            .bytes()
            .await
            .context("Failed to read image data")?
            .to_vec();

        // Notion にアップロード
        let file_upload_id = self
            .notion
            .upload_file(&attachment.filename, &content_type, data)
            .await
            .context("Failed to upload image to Notion")?;

        // 画像ブロックを追加して ID を返す
        self.notion
            .append_uploaded_image_block_with_id(page_id, &file_upload_id)
            .await
            .context("Failed to append uploaded image block")
    }
}

/// ファイル名から画像かどうかを判定する。
fn is_image(filename: &str) -> bool {
    let extensions = [".png", ".jpg", ".jpeg", ".gif", ".webp"];
    let lower = filename.to_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_with_valid_extensions() {
        assert!(is_image("photo.png"));
        assert!(is_image("photo.PNG"));
        assert!(is_image("image.jpg"));
        assert!(is_image("image.JPG"));
        assert!(is_image("picture.jpeg"));
        assert!(is_image("animation.gif"));
        assert!(is_image("modern.webp"));
    }

    #[test]
    fn test_is_image_rejects_similar_names() {
        // ドットなしの拡張子文字列で終わるファイル名は画像として判定されない
        assert!(!is_image("somepng"));
        assert!(!is_image("filejpg"));
        assert!(!is_image("imagejpeg"));
    }

    #[test]
    fn test_is_image_with_non_image_files() {
        assert!(!is_image("document.pdf"));
        assert!(!is_image("archive.zip"));
        assert!(!is_image("script.js"));
        assert!(!is_image("noextension"));
    }
}
