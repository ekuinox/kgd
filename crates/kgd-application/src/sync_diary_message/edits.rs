//! メッセージの編集・削除を Notion ブロックへ反映する処理。

use anyhow::Result;

use kgd_domain::{MessageBlock, SyncMessage, build_rich_text_and_url_blocks};

use super::SyncDiaryMessage;

impl SyncDiaryMessage {
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
}
