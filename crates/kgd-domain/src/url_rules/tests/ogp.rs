//! apply_ogp_to_bookmark のテスト。

use crate::ogp::OgpMetadata;

use super::*;

#[test]
fn test_apply_ogp_to_bookmark_with_title_and_description() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://example.com",
            "caption": []
        }
    });

    let ogp = OgpMetadata {
        title: Some("Example Title".to_string()),
        description: Some("Example Description".to_string()),
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    let caption = block["bookmark"]["caption"].as_array().unwrap();
    assert_eq!(caption.len(), 1);
    assert_eq!(
        caption[0]["text"]["content"],
        "Example Title\nExample Description"
    );
}

#[test]
fn test_apply_ogp_to_bookmark_with_title_only() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://example.com",
            "caption": []
        }
    });

    let ogp = OgpMetadata {
        title: Some("Title Only".to_string()),
        description: None,
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    let caption = block["bookmark"]["caption"].as_array().unwrap();
    assert_eq!(caption.len(), 1);
    assert_eq!(caption[0]["text"]["content"], "Title Only");
}

#[test]
fn test_apply_ogp_to_bookmark_with_description_only() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://example.com",
            "caption": []
        }
    });

    let ogp = OgpMetadata {
        title: None,
        description: Some("Description Only".to_string()),
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    let caption = block["bookmark"]["caption"].as_array().unwrap();
    assert_eq!(caption.len(), 1);
    assert_eq!(caption[0]["text"]["content"], "Description Only");
}

#[test]
fn test_apply_ogp_to_bookmark_empty_metadata() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://example.com",
            "caption": []
        }
    });

    let ogp = OgpMetadata {
        title: None,
        description: None,
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    // キャプションは変更されない（空のまま）
    let caption = block["bookmark"]["caption"].as_array().unwrap();
    assert!(caption.is_empty());
}

#[test]
fn test_apply_ogp_to_bookmark_long_description_truncated() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://example.com",
            "caption": []
        }
    });

    let long_description = "あ".repeat(250);
    let ogp = OgpMetadata {
        title: Some("Title".to_string()),
        description: Some(long_description),
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    let caption = block["bookmark"]["caption"].as_array().unwrap();
    let content = caption[0]["text"]["content"].as_str().unwrap();
    // Title + \n + 197文字 + "..."
    assert!(content.ends_with("..."));
    // 説明部分は 197 + 3 = 200 文字に切り詰められる
    let description_part = content.strip_prefix("Title\n").unwrap();
    assert_eq!(description_part.chars().count(), 200);
}
