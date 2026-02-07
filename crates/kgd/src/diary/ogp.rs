//! OGP メタデータの取得機能を提供する。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context as _, Result};
use regex::Regex;

/// OGP メタデータ。
#[derive(Debug, Clone, Default)]
pub struct OgpMetadata {
    /// og:title - ページタイトル
    pub title: Option<String>,
    /// og:description - ページ説明
    pub description: Option<String>,
}

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

/// HTML から OGP メタデータをパースする。
///
/// 正規表現を使用して meta タグから OGP 情報を抽出する。
fn parse_ogp_metadata(html: &str) -> OgpMetadata {
    let mut metadata = OgpMetadata::default();

    // og:title
    if let Some(value) = extract_meta_property(html, "og:title") {
        metadata.title = Some(value);
    }

    // og:description
    if let Some(value) = extract_meta_property(html, "og:description") {
        metadata.description = Some(value);
    }

    // フォールバック: <title> タグ
    if metadata.title.is_none()
        && let Some(value) = extract_title_tag(html)
    {
        metadata.title = Some(value);
    }

    // フォールバック: description meta タグ
    if metadata.description.is_none()
        && let Some(value) = extract_meta_name(html, "description")
    {
        metadata.description = Some(value);
    }

    metadata
}

/// property 属性で指定された meta タグの content を抽出する。
fn extract_meta_property(html: &str, property: &str) -> Option<String> {
    // <meta property="og:title" content="..."> または
    // <meta content="..." property="og:title"> のパターンに対応
    let pattern = format!(
        r#"<meta\s+(?:[^>]*?\s+)?property\s*=\s*["']{}["']\s+(?:[^>]*?\s+)?content\s*=\s*["']([^"']*)["']|<meta\s+(?:[^>]*?\s+)?content\s*=\s*["']([^"']*)["']\s+(?:[^>]*?\s+)?property\s*=\s*["']{}["']"#,
        regex::escape(property),
        regex::escape(property)
    );
    let re = Regex::new(&pattern).ok()?;

    if let Some(caps) = re.captures(html) {
        let content = caps.get(1).or_else(|| caps.get(2))?.as_str();
        let content = decode_html_entities(content.trim());
        if !content.is_empty() {
            return Some(content);
        }
    }
    None
}

/// name 属性で指定された meta タグの content を抽出する。
fn extract_meta_name(html: &str, name: &str) -> Option<String> {
    // <meta name="description" content="..."> または
    // <meta content="..." name="description"> のパターンに対応
    let pattern = format!(
        r#"<meta\s+(?:[^>]*?\s+)?name\s*=\s*["']{}["']\s+(?:[^>]*?\s+)?content\s*=\s*["']([^"']*)["']|<meta\s+(?:[^>]*?\s+)?content\s*=\s*["']([^"']*)["']\s+(?:[^>]*?\s+)?name\s*=\s*["']{}["']"#,
        regex::escape(name),
        regex::escape(name)
    );
    let re = Regex::new(&pattern).ok()?;

    if let Some(caps) = re.captures(html) {
        let content = caps.get(1).or_else(|| caps.get(2))?.as_str();
        let content = decode_html_entities(content.trim());
        if !content.is_empty() {
            return Some(content);
        }
    }
    None
}

/// <title> タグの内容を抽出する。
fn extract_title_tag(html: &str) -> Option<String> {
    let re = Regex::new(r"<title[^>]*>([^<]*)</title>").ok()?;

    if let Some(caps) = re.captures(html) {
        let title = caps.get(1)?.as_str();
        let title = decode_html_entities(title.trim());
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

/// 基本的な HTML エンティティをデコードする。
fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ogp_metadata_full() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="Test Title">
                <meta property="og:description" content="Test Description">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Test Title".to_string()));
        assert_eq!(metadata.description, Some("Test Description".to_string()));
    }

    #[test]
    fn test_parse_ogp_metadata_content_first() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta content="Title Content First" property="og:title">
                <meta content="Description Content First" property="og:description">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Title Content First".to_string()));
        assert_eq!(
            metadata.description,
            Some("Description Content First".to_string())
        );
    }

    #[test]
    fn test_parse_ogp_metadata_fallback_to_title_tag() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Fallback Title</title>
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Fallback Title".to_string()));
        assert_eq!(metadata.description, None);
    }

    #[test]
    fn test_parse_ogp_metadata_fallback_to_meta_description() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="OGP Title">
                <meta name="description" content="Meta Description">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("OGP Title".to_string()));
        assert_eq!(metadata.description, Some("Meta Description".to_string()));
    }

    #[test]
    fn test_parse_ogp_metadata_og_description_takes_priority() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:description" content="OGP Description">
                <meta name="description" content="Meta Description">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.description, Some("OGP Description".to_string()));
    }

    #[test]
    fn test_parse_ogp_metadata_empty_values_ignored() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="">
                <meta property="og:description" content="   ">
                <title>Fallback Title</title>
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Fallback Title".to_string()));
        assert_eq!(metadata.description, None);
    }

    #[test]
    fn test_parse_ogp_metadata_whitespace_trimmed() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="  Trimmed Title  ">
                <meta property="og:description" content="  Trimmed Description  ">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Trimmed Title".to_string()));
        assert_eq!(
            metadata.description,
            Some("Trimmed Description".to_string())
        );
    }

    #[test]
    fn test_parse_ogp_metadata_no_metadata() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head></head>
            <body>Hello</body>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, None);
        assert_eq!(metadata.description, None);
    }

    #[test]
    fn test_parse_ogp_metadata_html_entities_decoded() {
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta property="og:title" content="Title &amp; More">
                <meta property="og:description" content="&lt;Test&gt; &quot;Description&quot;">
            </head>
            </html>
        "#;

        let metadata = parse_ogp_metadata(html);
        assert_eq!(metadata.title, Some("Title & More".to_string()));
        assert_eq!(
            metadata.description,
            Some("<Test> \"Description\"".to_string())
        );
    }

    #[test]
    fn test_extract_title_tag() {
        assert_eq!(
            extract_title_tag("<title>Test</title>"),
            Some("Test".to_string())
        );
        assert_eq!(
            extract_title_tag("<title>  Trimmed  </title>"),
            Some("Trimmed".to_string())
        );
        assert_eq!(extract_title_tag("<title></title>"), None);
        assert_eq!(extract_title_tag("<p>No title</p>"), None);
    }
}
