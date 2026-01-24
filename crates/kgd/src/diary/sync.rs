//! Discord メッセージを Notion に同期する機能を提供する。

use anyhow::Result;
use serenity::model::channel::{Attachment, Message};

use super::NotionClient;

/// メッセージを Notion に同期するためのシンクロナイザー。
pub struct MessageSyncer<'a> {
    /// Notion クライアント
    notion: &'a NotionClient,
}

impl<'a> MessageSyncer<'a> {
    /// 新しい MessageSyncer を作成する。
    pub fn new(notion: &'a NotionClient) -> Self {
        Self { notion }
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
        // 画像の場合は画像ブロックとして追加
        if is_image(&attachment.filename) {
            // Discord の添付ファイル URL を直接使用
            self.notion
                .append_image_block(page_id, &attachment.url)
                .await?;
        } else {
            // その他のファイルはリンクとしてテキストブロックに追加
            let text = format!("[{}]({})", attachment.filename, attachment.url);
            self.notion.append_text_block(page_id, &text).await?;
        }

        Ok(())
    }
}

/// ファイル名から画像かどうかを判定する。
fn is_image(filename: &str) -> bool {
    let extensions = ["png", "jpg", "jpeg", "gif", "webp"];
    let lower = filename.to_lowercase();
    extensions.iter().any(|ext| lower.ends_with(ext))
}
