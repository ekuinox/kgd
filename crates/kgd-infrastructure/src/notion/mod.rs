//! Notion API との連携機能を提供する。

use anyhow::{Context as _, Result};
use notion_client::endpoints::Client;

use kgd_application::ports::NotionApi;

mod blocks;
mod files;
mod pages;
mod types;

pub use types::NotionTagConfig;

pub(crate) const NOTION_API_VERSION: &str = "2022-06-28";

/// Notion API クライアントのラッパー。
pub struct NotionClient {
    /// notion-client のクライアント
    pub(crate) client: Client,
    /// HTTP クライアント（ファイルアップロード用）
    pub(crate) http_client: reqwest::Client,
    /// Notion API トークン
    pub(crate) token: String,
    /// 日報を保存するデータベース ID
    pub(crate) database_id: String,
    /// タイトルプロパティ名
    pub(crate) title_property: String,
    /// ページ作成時に設定するタグ
    pub(crate) tags: Vec<NotionTagConfig>,
}

impl NotionClient {
    /// 新しい NotionClient を作成する。
    pub fn new(
        token: impl Into<String>,
        database_id: impl Into<String>,
        title_property: impl Into<String>,
        tags: Vec<NotionTagConfig>,
    ) -> Result<Self> {
        let token = token.into();
        let client = Client::new(token.clone(), None).context("Failed to create Notion client")?;
        let http_client = reqwest::Client::new();
        Ok(Self {
            client,
            http_client,
            token,
            database_id: database_id.into(),
            title_property: title_property.into(),
            tags,
        })
    }
}

#[async_trait::async_trait]
impl NotionApi for NotionClient {
    async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>> {
        NotionClient::find_diary_page_by_title(self, title).await
    }

    async fn create_diary_page(&self, title: &str) -> Result<(String, String)> {
        NotionClient::create_diary_page(self, title).await
    }

    async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String> {
        NotionClient::upload_file(self, filename, content_type, data).await
    }

    async fn append_blocks(
        &self,
        page_id: &str,
        children: Vec<serde_json::Value>,
    ) -> Result<Vec<String>> {
        NotionClient::append_blocks(self, page_id, children).await
    }

    async fn update_text_block(
        &self,
        block_id: &str,
        rich_text: Vec<serde_json::Value>,
    ) -> Result<()> {
        NotionClient::update_text_block(self, block_id, rich_text).await
    }

    async fn delete_block(&self, block_id: &str) -> Result<()> {
        NotionClient::delete_block(self, block_id).await
    }
}
