//! セグメント解析と出現順での Notion ブロック構築。

use regex::Regex;

use super::json::{
    bookmark_block_json, embed_block_json, inline_link_json, plain_text_chunks_json,
};
use super::matcher::{CompiledUrlRules, UrlBlockType};

/// URL 解析結果のブロック。出現順に並ぶ。
pub struct UrlParseResult {
    /// 出現順の Notion ブロック JSON と block_type 文字列のペア
    pub blocks: Vec<(serde_json::Value, String)>,
    /// Bookmark として処理された URL のリスト（OGP 取得対象）
    pub bookmark_urls: Vec<String>,
}

/// テキストからセグメントを解析し、出現順に Notion ブロックを生成する。
///
/// テキストやインラインリンクは paragraph ブロックにまとめ、
/// bookmark/embed が出現する位置で paragraph を分割して順序を保持する。
pub fn build_rich_text_and_url_blocks(text: &str, compiled: &CompiledUrlRules) -> UrlParseResult {
    let segments = parse_segments(text);
    let mut blocks: Vec<(serde_json::Value, String)> = Vec::new();
    let mut pending_rich_text: Vec<serde_json::Value> = Vec::new();
    let mut bookmark_urls: Vec<String> = Vec::new();

    for segment in segments {
        match segment {
            TextSegment::Plain(s) => {
                if !s.is_empty() {
                    pending_rich_text.extend(plain_text_chunks_json(&s));
                }
            }
            TextSegment::Url(url) => {
                let block_types = classify_url(&url, compiled);

                // インラインリンクは pending_rich_text に追加
                let has_link = block_types.contains(&UrlBlockType::Link);
                if has_link {
                    pending_rich_text.push(inline_link_json(&url));
                }

                // bookmark/embed の前に溜まった rich_text を paragraph として flush
                let has_standalone = block_types
                    .iter()
                    .any(|t| matches!(t, UrlBlockType::Bookmark | UrlBlockType::Embed));
                if has_standalone {
                    flush_paragraph(&mut pending_rich_text, &mut blocks);
                }

                for block_type in &block_types {
                    match block_type {
                        UrlBlockType::Link => {} // 上で処理済み
                        UrlBlockType::Bookmark => {
                            bookmark_urls.push(url.clone());
                            blocks.push((bookmark_block_json(&url), "bookmark".to_string()));
                        }
                        UrlBlockType::Embed => {
                            blocks.push((embed_block_json(&url), "embed".to_string()));
                        }
                    }
                }

                // いずれの変換も行われない場合のみプレーンテキストとして URL を表示
                if block_types.is_empty() {
                    pending_rich_text.extend(plain_text_chunks_json(&url));
                }
            }
        }
    }

    // 残りの rich_text を paragraph として追加
    flush_paragraph(&mut pending_rich_text, &mut blocks);

    UrlParseResult {
        blocks,
        bookmark_urls,
    }
}

/// 溜まった rich_text 要素を paragraph ブロックとして blocks に追加し、クリアする。
fn flush_paragraph(
    pending_rich_text: &mut Vec<serde_json::Value>,
    blocks: &mut Vec<(serde_json::Value, String)>,
) {
    if pending_rich_text.is_empty() {
        return;
    }
    let rich_text: Vec<serde_json::Value> = std::mem::take(pending_rich_text);
    blocks.push((
        serde_json::json!({
            "object": "block",
            "type": "paragraph",
            "paragraph": {
                "rich_text": rich_text
            }
        }),
        "text".to_string(),
    ));
}

/// テキストセグメントの種類。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextSegment {
    /// 通常のテキスト
    Plain(String),
    /// URL
    Url(String),
}

/// テキストを URL とプレーンテキストのセグメントに分割する。
pub(crate) fn parse_segments(text: &str) -> Vec<TextSegment> {
    let url_re = Regex::new(r"https?://[^\s<>\[\]()]+").unwrap();

    let mut segments = Vec::new();
    let mut last_end = 0;

    for m in url_re.find_iter(text) {
        if m.start() > last_end {
            segments.push(TextSegment::Plain(text[last_end..m.start()].to_string()));
        }
        segments.push(TextSegment::Url(m.as_str().to_string()));
        last_end = m.end();
    }

    if last_end < text.len() {
        segments.push(TextSegment::Plain(text[last_end..].to_string()));
    }

    segments
}

/// URL にマッチするルールのブロックタイプ一覧を返す。
///
/// 最初にマッチしたルールのみ適用。どのルールにもマッチしなかった場合は
/// デフォルトの変換タイプを返す。
pub(crate) fn classify_url(url: &str, compiled: &CompiledUrlRules) -> Vec<UrlBlockType> {
    for rule in &compiled.rules {
        if rule.matcher.is_match(url) {
            return rule.block_types.clone();
        }
    }
    compiled.default_types.clone()
}
