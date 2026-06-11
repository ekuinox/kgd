//! reqwest による AttachmentDownloader ポートの実装。

use anyhow::{Context as _, Result};

use kgd_application::ports::AttachmentDownloader;
use kgd_domain::{SyncAttachment, guess_content_type};

/// reqwest で添付ファイルをダウンロードする [`AttachmentDownloader`] 実装。
pub struct ReqwestDownloader {
    /// HTTP クライアント
    http_client: reqwest::Client,
}

impl ReqwestDownloader {
    /// 新しい ReqwestDownloader を作成する。
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }
}

impl Default for ReqwestDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AttachmentDownloader for ReqwestDownloader {
    async fn download(&self, attachment: &SyncAttachment) -> Result<(Vec<u8>, String)> {
        let response = self
            .http_client
            .get(&attachment.url)
            .send()
            .await
            .context("Failed to download file from Discord")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download file: status = {}", response.status());
        }

        let header_content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        // Discord が返す Content-Type が汎用的な場合、ファイル名の拡張子から推定する
        let content_type = if header_content_type == "application/octet-stream"
            || header_content_type.is_empty()
        {
            guess_content_type(&attachment.filename).unwrap_or(header_content_type)
        } else {
            header_content_type
        };

        let data = response
            .bytes()
            .await
            .context("Failed to read file data")?
            .to_vec();

        Ok((data, content_type))
    }
}
