//! Notion ブロック JSON の生成と OGP キャプションの適用。

use crate::ogp::OgpMetadata;

/// プレーンテキストの rich_text JSON を生成する。
pub(super) fn plain_text_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "text": {
            "content": text
        }
    })
}

/// インラインリンクの rich_text JSON を生成する。
pub(super) fn inline_link_json(url: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "text": {
            "content": url,
            "link": {
                "url": url
            }
        }
    })
}

/// ブックマークブロック JSON を生成する。
pub(super) fn bookmark_block_json(url: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": url,
            "caption": []
        }
    })
}

/// 埋め込みブロック JSON を生成する。
pub(super) fn embed_block_json(url: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "embed",
        "embed": {
            "url": url
        }
    })
}

/// OGP メタデータをブックマークブロックに適用する。
///
/// タイトルと説明をキャプションとして設定する。
pub fn apply_ogp_to_bookmark(block_json: &mut serde_json::Value, ogp: &OgpMetadata) {
    let caption = build_bookmark_caption(ogp);
    if !caption.is_empty() {
        block_json["bookmark"]["caption"] = serde_json::json!(caption);
    }
}

/// OGP メタデータからブックマークキャプションを構築する。
fn build_bookmark_caption(ogp: &OgpMetadata) -> Vec<serde_json::Value> {
    let mut parts = Vec::new();

    // タイトルを追加
    if let Some(title) = &ogp.title {
        parts.push(title.clone());
    }

    // 説明を追加（タイトルがある場合は改行で区切る）
    if let Some(description) = &ogp.description {
        if !parts.is_empty() {
            parts.push("\n".to_string());
        }
        // 説明が長すぎる場合は切り詰める
        let truncated = if description.chars().count() > 200 {
            format!("{}...", description.chars().take(197).collect::<String>())
        } else {
            description.clone()
        };
        parts.push(truncated);
    }

    if parts.is_empty() {
        return vec![];
    }

    let content = parts.join("");
    vec![serde_json::json!({
        "type": "text",
        "text": {
            "content": content
        }
    })]
}
