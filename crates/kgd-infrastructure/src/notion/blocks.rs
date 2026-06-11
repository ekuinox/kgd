//! ブロックの追加・更新・削除。

use anyhow::{Context as _, Result, bail};

use super::{NOTION_API_VERSION, NotionClient, types::AppendBlockChildrenResponse};

impl NotionClient {
    /// 複数のブロックを一括でページに追加し、作成されたブロック ID のリストを返す。
    pub async fn append_blocks(
        &self,
        page_id: &str,
        children: Vec<serde_json::Value>,
    ) -> Result<Vec<String>> {
        if children.is_empty() {
            return Ok(vec![]);
        }

        let body = serde_json::json!({ "children": children });

        let response = self
            .http_client
            .patch(format!(
                "https://api.notion.com/v1/blocks/{}/children",
                page_id
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to append blocks")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to append blocks: {} - {}", status, body);
        }

        let result: AppendBlockChildrenResponse = response
            .json()
            .await
            .context("Failed to parse append block response")?;

        Ok(result.results.into_iter().map(|b| b.id).collect())
    }

    /// テキストブロックを更新する。
    pub async fn update_text_block(
        &self,
        block_id: &str,
        rich_text: Vec<serde_json::Value>,
    ) -> Result<()> {
        let body = serde_json::json!({
            "paragraph": {
                "rich_text": rich_text
            }
        });

        let response = self
            .http_client
            .patch(format!("https://api.notion.com/v1/blocks/{}", block_id))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to update block")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to update block: {} - {}", status, body);
        }

        Ok(())
    }

    /// ブロックを削除する。
    pub async fn delete_block(&self, block_id: &str) -> Result<()> {
        let response = self
            .http_client
            .delete(format!("https://api.notion.com/v1/blocks/{}", block_id))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .send()
            .await
            .context("Failed to delete block")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to delete block: {} - {}", status, body);
        }

        Ok(())
    }
}
