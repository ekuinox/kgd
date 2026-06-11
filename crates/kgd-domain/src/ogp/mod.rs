//! OGP メタデータの型と HTML 解析の純粋ロジック。

use regex::Regex;

/// OGP メタデータ。
#[derive(Debug, Clone, Default)]
pub struct OgpMetadata {
    /// og:title - ページタイトル
    pub title: Option<String>,
    /// og:description - ページ説明
    pub description: Option<String>,
}

/// HTML から OGP メタデータをパースする。
///
/// 正規表現を使用して meta タグから OGP 情報を抽出する。
pub fn parse_ogp_metadata(html: &str) -> OgpMetadata {
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
mod tests;
