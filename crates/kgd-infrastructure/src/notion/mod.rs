//! Notion API との連携機能を提供する。

use std::time::Duration;

use anyhow::{Context as _, Result};
use reqwest::{Method, RequestBuilder};

use kgd_application::ports::NotionApi;

use self::retry::{RetryScope, retry};

mod blocks;
mod files;
mod pages;
mod retry;
mod types;

pub use types::NotionTagConfig;

pub(crate) const NOTION_API_VERSION: &str = "2022-06-28";

/// 接続確立のタイムアウト。
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// リクエスト全体のタイムアウト。添付ファイルのアップロードを考慮して長めに取る。
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// アイドル接続を保持する時間。
///
/// サーバー側に切断された接続を再利用して `Connection reset by peer` になるのを避けるため、
/// 既定値より短くしている。
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Notion API クライアントのラッパー。
pub struct NotionClient {
    /// HTTP クライアント
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
        let http_client = http_client_builder()
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self {
            http_client,
            token,
            database_id: database_id.into(),
            title_property: title_property.into(),
            tags,
        })
    }

    /// 認証情報と API バージョンを設定したリクエストを組み立てる。
    pub(crate) fn request(&self, method: Method, url: impl reqwest::IntoUrl) -> RequestBuilder {
        self.http_client
            .request(method, url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Notion-Version", NOTION_API_VERSION)
    }
}

/// Notion API 用の HTTP クライアントの共通設定を組み立てる。
fn http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
}

/// 一時的な通信障害に対しては再試行してから呼び出し元へ結果を返す。
///
/// 再実行すると副作用が重複する操作は、リクエストがサーバーに届いていないことが
/// 確実な場合 ([`RetryScope::ConnectOnly`]) のみ再試行する。
#[async_trait::async_trait]
impl NotionApi for NotionClient {
    async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>> {
        retry("find_diary_page_by_title", RetryScope::Transient, |_| {
            NotionClient::find_diary_page_by_title(self, title)
        })
        .await
    }

    async fn create_diary_page(&self, title: &str) -> Result<(String, String)> {
        retry(
            "create_diary_page",
            RetryScope::Transient,
            |attempt| async move {
                // 再試行時は、前回の試行でページが作られていないかを先に確認して重複作成を避ける
                if attempt > 0
                    && let Some(page) = NotionClient::find_diary_page_by_title(self, title).await?
                {
                    return Ok(page);
                }
                NotionClient::create_diary_page(self, title).await
            },
        )
        .await
    }

    async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String> {
        retry("upload_file", RetryScope::ConnectOnly, |_| {
            NotionClient::upload_file(self, filename, content_type, data.clone())
        })
        .await
    }

    async fn append_blocks(
        &self,
        page_id: &str,
        children: Vec<serde_json::Value>,
    ) -> Result<Vec<String>> {
        retry("append_blocks", RetryScope::ConnectOnly, |_| {
            NotionClient::append_blocks(self, page_id, children.clone())
        })
        .await
    }

    async fn update_text_block(
        &self,
        block_id: &str,
        rich_text: Vec<serde_json::Value>,
    ) -> Result<()> {
        retry("update_text_block", RetryScope::ConnectOnly, |_| {
            NotionClient::update_text_block(self, block_id, rich_text.clone())
        })
        .await
    }

    async fn delete_block(&self, block_id: &str) -> Result<()> {
        retry("delete_block", RetryScope::ConnectOnly, |_| {
            NotionClient::delete_block(self, block_id)
        })
        .await
    }
}
