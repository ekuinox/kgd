//! ブロックの追加・更新・削除。

use anyhow::{Context as _, Result};
use reqwest::Method;

use super::{NotionClient, retry::ensure_success, types::AppendBlockChildrenResponse};

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
            .request(Method::PATCH, format!("/blocks/{}/children", page_id))
            .json(&body)
            .send()
            .await
            .context("Failed to append blocks")?;

        let response = ensure_success(response, "Failed to append blocks").await?;

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
            .request(Method::PATCH, format!("/blocks/{}", block_id))
            .json(&body)
            .send()
            .await
            .context("Failed to update block")?;

        ensure_success(response, "Failed to update block").await?;

        Ok(())
    }

    /// ブロックを削除する。
    pub async fn delete_block(&self, block_id: &str) -> Result<()> {
        let response = self
            .request(Method::DELETE, format!("/blocks/{}", block_id))
            .send()
            .await
            .context("Failed to delete block")?;

        ensure_success(response, "Failed to delete block").await?;

        Ok(())
    }
}
