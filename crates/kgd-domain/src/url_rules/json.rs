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

/// プレーンテキストを Notion の上限に収まる複数の rich_text JSON に分割する。
///
/// Notion は rich_text 1 要素あたり 2000 文字までしか受け付けない。
/// Discord は Nitro で 4000 文字まで投稿できるため、そのまま送ると
/// append_blocks が 400 validation_error で失敗する。
/// 切り詰めると本文が失われるので、分割して全文を保持する。
pub(super) fn plain_text_chunks_json(text: &str) -> Vec<serde_json::Value> {
    if text.chars().count() <= NOTION_RICH_TEXT_MAX_CHARS {
        return vec![plain_text_json(text)];
    }

    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(NOTION_RICH_TEXT_MAX_CHARS)
        .map(|chunk| plain_text_json(&chunk.iter().collect::<String>()))
        .collect()
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

/// キャプションのタイトル・説明それぞれの最大文字数。
const CAPTION_PART_MAX_CHARS: usize = 200;

/// Notion の rich_text 1 要素あたりの最大文字数。
const NOTION_RICH_TEXT_MAX_CHARS: usize = 2000;

/// 文字列を指定文字数以内に切り詰める。超える場合は末尾を "..." にする。
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{head}...")
}

/// OGP メタデータからブックマークキャプションを構築する。
///
/// Instagram のように og:title へ本文全体を入れるサイトがあるため、
/// タイトルと説明の両方を切り詰め、最終的に Notion の上限にも収める。
fn build_bookmark_caption(ogp: &OgpMetadata) -> Vec<serde_json::Value> {
    let mut parts = Vec::new();

    // タイトルを追加
    if let Some(title) = &ogp.title {
        parts.push(truncate_chars(title, CAPTION_PART_MAX_CHARS));
    }

    // 説明を追加（タイトルがある場合は改行で区切る）
    if let Some(description) = &ogp.description {
        if !parts.is_empty() {
            parts.push("\n".to_string());
        }
        parts.push(truncate_chars(description, CAPTION_PART_MAX_CHARS));
    }

    if parts.is_empty() {
        return vec![];
    }

    let content = truncate_chars(&parts.join(""), NOTION_RICH_TEXT_MAX_CHARS);
    vec![serde_json::json!({
        "type": "text",
        "text": {
            "content": content
        }
    })]
}
