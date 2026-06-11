//! OGP メタデータ取得の OgpClient ポート実装。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context as _, Result};
use kgd_application::ports::OgpClient;
use kgd_domain::{OgpMetadata, parse_ogp_metadata};

/// OGP メタデータを取得するクライアント。
pub struct OgpFetcher {
    http_client: reqwest::Client,
}

impl OgpFetcher {
    /// 新しい OgpFetcher を作成する。
    pub fn new(timeout: Duration) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("kgd-bot/1.0")
            .build()
            .context("Failed to create HTTP client for OGP fetcher")?;

        Ok(Self { http_client })
    }

    /// URL から OGP メタデータを取得する。
    ///
    /// 取得に失敗した場合は None を返す（エラーはログに記録）。
    pub async fn fetch(&self, url: &str) -> Option<OgpMetadata> {
        match self.fetch_inner(url).await {
            Ok(metadata) => Some(metadata),
            Err(e) => {
                tracing::debug!(url = %url, error = %e, "Failed to fetch OGP metadata");
                None
            }
        }
    }

    /// 複数の URL から OGP メタデータを並列で取得する。
    pub async fn fetch_many(&self, urls: &[String]) -> HashMap<String, OgpMetadata> {
        let futures: Vec<_> = urls
            .iter()
            .map(|url| async {
                let metadata = self.fetch(url).await;
                (url.clone(), metadata)
            })
            .collect();

        futures::future::join_all(futures)
            .await
            .into_iter()
            .filter_map(|(url, ogp)| ogp.map(|o| (url, o)))
            .collect()
    }

    async fn fetch_inner(&self, url: &str) -> Result<OgpMetadata> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .context("HTTP request failed")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP status: {}", response.status());
        }

        let html = response
            .text()
            .await
            .context("Failed to read response body")?;

        Ok(parse_ogp_metadata(&html))
    }
}

#[async_trait::async_trait]
impl OgpClient for OgpFetcher {
    async fn fetch_many(&self, urls: &[String]) -> HashMap<String, OgpMetadata> {
        OgpFetcher::fetch_many(self, urls).await
    }
}
