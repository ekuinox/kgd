//! apply_ogp_to_bookmark のテスト。

use crate::ogp::OgpMetadata;

use super::*;

/// OGP にタイトルと説明の両方がある場合、bookmark の caption が
/// 「タイトル\n説明」の 1 要素として設定されることを確認する。
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

/// OGP にタイトルのみある場合、bookmark の caption がタイトル文字列のみになることを確認する。
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

/// OGP に説明のみある場合、bookmark の caption が説明文字列のみになることを確認する。
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

/// OGP にタイトルも説明も無い場合、bookmark の caption が変更されず空のままになることを確認する。
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

/// 説明が長すぎる場合、説明部分が末尾 "..." を含む 200 文字に切り詰められて
/// caption に設定されることを確認する。
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

/// タイトルが長すぎる場合も切り詰められることを確認する。
///
/// Instagram のように og:title へ投稿本文全体が入るサイトがあり、
/// 切り詰めないと Notion の caption 上限 (2000 文字) を超えて 400 になる。
#[test]
fn test_apply_ogp_to_bookmark_long_title_truncated() {
    let mut block = serde_json::json!({
        "object": "block",
        "type": "bookmark",
        "bookmark": {
            "url": "https://www.instagram.com/p/DcQylQzn4Wx/",
            "caption": []
        }
    });

    let ogp = OgpMetadata {
        title: Some("あ".repeat(2900)),
        description: Some("い".repeat(250)),
    };

    apply_ogp_to_bookmark(&mut block, &ogp);

    let caption = block["bookmark"]["caption"].as_array().unwrap();
    let content = caption[0]["text"]["content"].as_str().unwrap();
    assert!(
        content.chars().count() <= 2000,
        "caption は Notion の上限 2000 文字以内に収まるべき: {}",
        content.chars().count()
    );

    let title_part = content.split('\n').next().unwrap();
    assert_eq!(title_part.chars().count(), 200);
    assert!(title_part.ends_with("..."));
}
