//! build_rich_text_and_url_blocks の単一 URL 変換テスト。

use regex::Regex;

use super::*;

#[test]
fn test_build_no_urls() {
    let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
    let result = build_rich_text_and_url_blocks("plain text", &compiled);
    assert_eq!(result.blocks.len(), 1);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 1);
    assert_eq!(rich_text[0]["text"]["content"], "plain text");
    assert!(rich_text[0]["text"]["link"].is_null());
}

#[test]
fn test_build_inline_link_default() {
    let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
    let result = build_rich_text_and_url_blocks("see https://example.com here", &compiled);
    // すべてインラインなので paragraph 1 つ
    assert_eq!(result.blocks.len(), 1);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 3);
    assert_eq!(rich_text[1]["text"]["content"], "https://example.com");
    assert_eq!(rich_text[1]["text"]["link"]["url"], "https://example.com");
}

#[test]
fn test_build_url_no_default_renders_plain_text() {
    let compiled = compiled_with_rules(vec![]);
    let result = build_rich_text_and_url_blocks("see https://example.com here", &compiled);
    assert_eq!(result.blocks.len(), 1);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 3);
    // URL はプレーンテキストとして表示（リンクなし）
    assert_eq!(rich_text[1]["text"]["content"], "https://example.com");
    assert!(rich_text[1]["text"]["link"].is_null());
}

#[test]
fn test_build_bookmark_only_no_link() {
    let compiled = compiled_with_default(
        vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }],
        vec![UrlBlockType::Link],
    );
    let result = build_rich_text_and_url_blocks("check https://github.com/ekuinox/kgd", &compiled);
    // "check " → paragraph, URL → bookmark の順
    assert_eq!(result.blocks.len(), 2);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 1);
    assert_eq!(rich_text[0]["text"]["content"], "check ");
    assert_eq!(result.blocks[1].1, "bookmark");
    assert_eq!(
        result.blocks[1].0["bookmark"]["url"],
        "https://github.com/ekuinox/kgd"
    );
}

#[test]
fn test_build_link_and_bookmark() {
    let compiled = compiled_with_default(
        vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Link, UrlBlockType::Bookmark],
        }],
        vec![UrlBlockType::Link],
    );
    let result = build_rich_text_and_url_blocks("check https://github.com/ekuinox/kgd", &compiled);
    // "check " + inline link → paragraph, bookmark の順
    assert_eq!(result.blocks.len(), 2);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 2);
    assert_eq!(
        rich_text[1]["text"]["link"]["url"],
        "https://github.com/ekuinox/kgd"
    );
    assert_eq!(result.blocks[1].1, "bookmark");
}

#[test]
fn test_build_embed_rule() {
    let compiled = compiled_with_rules(vec![UrlRule {
        matcher: UrlMatcher::Regex(Regex::new(r"https://youtube\.com/watch.*").unwrap()),
        block_types: vec![UrlBlockType::Embed],
    }]);
    let result = build_rich_text_and_url_blocks("https://youtube.com/watch?v=abc", &compiled);
    // embed のみ、paragraph なし
    assert_eq!(result.blocks.len(), 1);
    assert_eq!(result.blocks[0].1, "embed");
    assert_eq!(
        result.blocks[0].0["embed"]["url"],
        "https://youtube.com/watch?v=abc"
    );
}
