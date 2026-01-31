//! Notion API との連携機能を提供する。

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};
use notion_client::{
    endpoints::Client,
    objects::{
        page::{PageProperty, SelectPropertyValue},
        parent::Parent,
        rich_text::{RichText, Text},
    },
};
use reqwest::multipart;
use serde::{Deserialize, Serialize};

use crate::config::NotionTagConfig;

const NOTION_API_VERSION: &str = "2022-06-28";

/// Notion API クライアントのラッパー。
pub struct NotionClient {
    /// notion-client のクライアント
    client: Client,
    /// HTTP クライアント（ファイルアップロード用）
    http_client: reqwest::Client,
    /// Notion API トークン
    token: String,
    /// 日報を保存するデータベース ID
    database_id: String,
    /// タイトルプロパティ名
    title_property: String,
    /// ページ作成時に設定するタグ
    tags: Vec<NotionTagConfig>,
}

/// ファイルアップロードのレスポンス。
#[derive(Debug, Deserialize)]
struct FileUploadResponse {
    id: String,
    status: String,
}

/// ファイルアップロードのリクエストボディ。
#[derive(Debug, Serialize)]
struct CreateFileUploadRequest {
    mode: String,
    filename: String,
    content_type: String,
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

    /// 指定したタイトルの日報ページを検索し、存在すればページ ID と URL を返す。
    pub async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>> {
        let body = serde_json::json!({
            "filter": {
                "property": self.title_property,
                "title": {
                    "equals": title
                }
            },
            "page_size": 1
        });

        let response = self
            .http_client
            .post(format!(
                "https://api.notion.com/v1/databases/{}/query",
                self.database_id
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to query database")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("Failed to query database: {} - {}", status, body);
        }

        let result: DatabaseQueryResponse = response
            .json()
            .await
            .context("Failed to parse database query response")?;

        Ok(result
            .results
            .first()
            .map(|page| (page.id.clone(), page.url.clone())))
    }

    /// 日報ページを作成し、ページ ID と URL を返す。
    pub async fn create_diary_page(&self, title: &str) -> Result<(String, String)> {
        let mut properties = BTreeMap::new();

        // タイトルプロパティを設定
        properties.insert(
            self.title_property.clone(),
            PageProperty::Title {
                id: None,
                title: vec![RichText::Text {
                    text: Text {
                        content: title.to_string(),
                        link: None,
                    },
                    annotations: None,
                    plain_text: None,
                    href: None,
                }],
            },
        );

        // タグ（セレクト/マルチセレクトプロパティ）を設定
        for tag in &self.tags {
            let select_value = SelectPropertyValue {
                id: None,
                name: Some(tag.value.clone()),
                color: None,
            };
            let property = if tag.multi_select {
                PageProperty::MultiSelect {
                    id: None,
                    multi_select: vec![select_value],
                }
            } else {
                PageProperty::Select {
                    id: None,
                    select: Some(select_value),
                }
            };
            properties.insert(tag.property.clone(), property);
        }

        let request = notion_client::endpoints::pages::create::request::CreateAPageRequest {
            parent: Parent::DatabaseId {
                database_id: self.database_id.clone(),
            },
            properties,
            ..Default::default()
        };

        let page = self
            .client
            .pages
            .create_a_page(request)
            .await
            .context("Failed to create Notion page")?;

        Ok((page.id, page.url))
    }

    /// ファイルをNotionにアップロードし、ファイルアップロードIDを返す。
    pub async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String> {
        // 1. Create file upload
        let create_request = CreateFileUploadRequest {
            mode: "single_part".to_string(),
            filename: filename.to_string(),
            content_type: content_type.to_string(),
        };

        let create_response = self
            .http_client
            .post("https://api.notion.com/v1/file_uploads")
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .json(&create_request)
            .send()
            .await
            .context("Failed to create file upload")?;

        if !create_response.status().is_success() {
            let status = create_response.status();
            let body = create_response.text().await.unwrap_or_default();
            bail!("Failed to create file upload: {} - {}", status, body);
        }

        let file_upload: FileUploadResponse = create_response
            .json()
            .await
            .context("Failed to parse file upload response")?;

        let file_upload_id = file_upload.id;

        // 2. Send file content
        let part = multipart::Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(content_type)
            .context("Invalid content type")?;

        let form = multipart::Form::new().part("file", part);

        let send_response = self
            .http_client
            .post(format!(
                "https://api.notion.com/v1/file_uploads/{}/send",
                file_upload_id
            ))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
            .multipart(form)
            .send()
            .await
            .context("Failed to send file upload")?;

        if !send_response.status().is_success() {
            let status = send_response.status();
            let body = send_response.text().await.unwrap_or_default();
            bail!("Failed to send file upload: {} - {}", status, body);
        }

        let upload_result: FileUploadResponse = send_response
            .json()
            .await
            .context("Failed to parse send response")?;

        if upload_result.status != "uploaded" {
            bail!(
                "File upload not completed: status = {}",
                upload_result.status
            );
        }

        Ok(file_upload_id)
    }

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

/// ブロック追加レスポンスのブロック情報。
#[derive(Debug, Deserialize)]
struct BlockInfo {
    id: String,
}

/// ブロック追加レスポンス。
#[derive(Debug, Deserialize)]
struct AppendBlockChildrenResponse {
    results: Vec<BlockInfo>,
}

/// データベースクエリレスポンスのページ情報。
#[derive(Debug, Deserialize)]
struct PageInfo {
    id: String,
    url: String,
}

/// データベースクエリレスポンス。
#[derive(Debug, Deserialize)]
struct DatabaseQueryResponse {
    results: Vec<PageInfo>,
}
