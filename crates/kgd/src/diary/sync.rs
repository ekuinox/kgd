//! Discord メッセージを Notion に同期する機能を提供する。

use anyhow::{Context as _, Result};
use serenity::model::channel::{Attachment, Message};

use super::NotionClient;

/// メッセージを Notion に同期するためのシンクロナイザー。
pub struct MessageSyncer<'a> {
    /// Notion クライアント
    notion: &'a NotionClient,
    /// HTTP クライアント（画像ダウンロード用）
    http_client: reqwest::Client,
}

impl<'a> MessageSyncer<'a> {
    /// 新しい MessageSyncer を作成する。
    pub fn new(notion: &'a NotionClient) -> Self {
        Self {
            notion,
            http_client: reqwest::Client::new(),
        }
    }

    /// メッセージを Notion ページに同期する。
    ///
    /// # Returns
    /// 同期が成功した場合は `Ok(true)`、スキップした場合は `Ok(false)`
    pub async fn sync_message(&self, page_id: &str, message: &Message) -> Result<bool> {
        let has_content = !message.content.is_empty();
        let has_attachments = !message.attachments.is_empty();

        if !has_content && !has_attachments {
            return Ok(false);
        }

        // テキストコンテンツを同期
        if has_content {
            self.notion
                .append_text_block(page_id, &message.content)
                .await?;
        }

        // 添付ファイルを同期
        for attachment in &message.attachments {
            self.sync_attachment(page_id, attachment).await?;
        }

        Ok(true)
    }

    /// 添付ファイルを Notion に同期する。
    async fn sync_attachment(&self, page_id: &str, attachment: &Attachment) -> Result<()> {
        // 画像の場合はダウンロードしてNotionにアップロード
        if is_image(&attachment.filename) {
            self.upload_image(page_id, attachment).await?;
        } else {
            // その他のファイルはリンクとしてテキストブロックに追加
            let text = format!("[{}]({})", attachment.filename, attachment.url);
            self.notion.append_text_block(page_id, &text).await?;
        }

        Ok(())
    }

    /// 画像をダウンロードしてNotionにアップロードする。
    async fn upload_image(&self, page_id: &str, attachment: &Attachment) -> Result<()> {
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

        // 画像ブロックを追加
        self.notion
            .append_uploaded_image_block(page_id, &file_upload_id)
            .await
            .context("Failed to append uploaded image block")?;

        Ok(())
    }
}

/// ファイル名から画像かどうかを判定する。
// TODO: 拡張子の前にドットを含めるべき (e.g., ".png") - "somepng" のような誤判定を防ぐ
fn is_image(filename: &str) -> bool {
    let extensions = ["png", "jpg", "jpeg", "gif", "webp"];
    let lower = filename.to_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}
