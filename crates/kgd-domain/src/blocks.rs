//! Notion ブロック JSON を生成する純粋ロジック。

use crate::url_rules::plain_text_chunks_json;

/// アップロード済み画像の画像ブロック JSON を生成する。
pub fn image_block_json(file_upload_id: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "image",
        "image": {
            "type": "file_upload",
            "file_upload": {
                "id": file_upload_id
            }
        }
    })
}

/// アップロード済みファイルのファイルブロック JSON を生成する。
pub fn file_block_json(file_upload_id: &str, filename: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "file",
        "file": {
            "type": "file_upload",
            "file_upload": {
                "id": file_upload_id
            },
            "name": filename
        }
    })
}

/// 子ブロックを折りたたむトグルブロック JSON を生成する。
pub fn toggle_block_json(summary: &str, children: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "toggle",
        "toggle": {
            "rich_text": [{
                "type": "text",
                "text": {
                    "content": summary
                }
            }],
            "children": children
        }
    })
}

/// プレーンテキストの段落ブロック JSON を生成する。
///
/// Notion の rich_text 1 要素あたりの上限を超える本文は、複数の要素に分割する。
pub fn paragraph_block_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "paragraph",
        "paragraph": {
            "rich_text": plain_text_chunks_json(text)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 子ブロックを渡してトグルブロック JSON を生成し、type が toggle、
    /// summary が rich_text に入り、子ブロック（image）が children に
    /// 1 件含まれることを確認する。
    #[test]
    fn test_toggle_block_json_includes_children() {
        let toggle = toggle_block_json("Spoiler image", vec![image_block_json("upload-id")]);

        assert_eq!(toggle["type"], "toggle");
        assert_eq!(
            toggle["toggle"]["rich_text"][0]["text"]["content"],
            "Spoiler image"
        );
        let children = toggle["toggle"]["children"]
            .as_array()
            .expect("toggle children should be an array");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0]["type"], "image");
    }

    /// 段落ブロック JSON が type paragraph で、本文を rich_text に持つことを確認する。
    #[test]
    fn paragraph_block_json_holds_text() {
        let block = paragraph_block_json("位置ログ\n移動距離 1.0 km");

        assert_eq!(block["type"], "paragraph");
        assert_eq!(
            block["paragraph"]["rich_text"][0]["text"]["content"],
            "位置ログ\n移動距離 1.0 km"
        );
    }

    /// Notion の上限 (2000 文字) を超える本文が複数の rich_text 要素に分割されることを確認する。
    #[test]
    fn paragraph_block_json_splits_long_text() {
        let text = "あ".repeat(2500);

        let block = paragraph_block_json(&text);

        assert_eq!(block["paragraph"]["rich_text"].as_array().unwrap().len(), 2);
    }
}
