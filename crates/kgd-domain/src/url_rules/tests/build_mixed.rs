//! build_rich_text_and_url_blocks の複数 URL・順序・bookmark_urls のテスト。

use regex::Regex;

use super::*;

#[test]
fn test_build_multiple_block_types() {
    let compiled = compiled_with_rules(vec![UrlRule {
        matcher: UrlMatcher::Regex(Regex::new(r"https://youtube\.com/watch.*").unwrap()),
        block_types: vec![
            UrlBlockType::Link,
            UrlBlockType::Bookmark,
            UrlBlockType::Embed,
        ],
    }]);
    let result = build_rich_text_and_url_blocks("https://youtube.com/watch?v=abc", &compiled);
    // inline link → paragraph が flush され、bookmark, embed が続く
    assert_eq!(result.blocks.len(), 3);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(
        rich_text[0]["text"]["link"]["url"],
        "https://youtube.com/watch?v=abc"
    );
    assert_eq!(result.blocks[1].1, "bookmark");
    assert_eq!(result.blocks[2].1, "embed");
}

#[test]
fn test_build_mixed_urls() {
    let compiled = compiled_with_default(
        vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }],
        vec![UrlBlockType::Link],
    );
    let result = build_rich_text_and_url_blocks(
        "see https://example.com and https://github.com/ekuinox/kgd",
        &compiled,
    );
    // "see " + inline link(example.com) + " and " → paragraph, bookmark(github.com)
    assert_eq!(result.blocks.len(), 2);
    assert_eq!(result.blocks[0].1, "text");
    let rich_text = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rich_text.len(), 3);
    assert_eq!(rich_text[0]["text"]["content"], "see ");
    assert_eq!(rich_text[1]["text"]["link"]["url"], "https://example.com");
    assert_eq!(rich_text[2]["text"]["content"], " and ");
    assert_eq!(result.blocks[1].1, "bookmark");
}

#[test]
fn test_build_order_text_bookmark_text() {
    let compiled = compiled_with_default(
        vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }],
        vec![UrlBlockType::Link],
    );
    let result = build_rich_text_and_url_blocks("before https://github.com/foo after", &compiled);
    // "before " → paragraph, bookmark, " after" → paragraph
    assert_eq!(result.blocks.len(), 3);
    assert_eq!(result.blocks[0].1, "text");
    let rt0 = result.blocks[0].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rt0[0]["text"]["content"], "before ");
    assert_eq!(result.blocks[1].1, "bookmark");
    assert_eq!(
        result.blocks[1].0["bookmark"]["url"],
        "https://github.com/foo"
    );
    assert_eq!(result.blocks[2].1, "text");
    let rt2 = result.blocks[2].0["paragraph"]["rich_text"]
        .as_array()
        .unwrap();
    assert_eq!(rt2[0]["text"]["content"], " after");
}

#[test]
fn test_build_bookmark_urls_collected() {
    let compiled = compiled_with_default(
        vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }],
        vec![UrlBlockType::Link],
    );
    let result = build_rich_text_and_url_blocks(
        "check https://github.com/foo and https://github.com/bar",
        &compiled,
    );
    assert_eq!(result.bookmark_urls.len(), 2);
    assert!(
        result
            .bookmark_urls
            .contains(&"https://github.com/foo".to_string())
    );
    assert!(
        result
            .bookmark_urls
            .contains(&"https://github.com/bar".to_string())
    );
}

#[test]
fn test_build_bookmark_urls_empty_for_links_only() {
    let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
    let result = build_rich_text_and_url_blocks("check https://example.com", &compiled);
    assert!(result.bookmark_urls.is_empty());
}
