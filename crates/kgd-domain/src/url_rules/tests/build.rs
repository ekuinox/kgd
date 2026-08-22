//! build_rich_text_and_url_blocks の単一 URL 変換テスト。

use regex::Regex;

use super::*;

/// URL を含まないテキストが、1 つの paragraph ブロック (リンクなしのプレーン rich_text) に
/// 変換されることを確認する。
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

/// デフォルト変換が Link の場合、URL を含むテキストが 1 つの paragraph にまとまり、
/// URL 部分がインラインリンク (link.url 付き) になることを確認する。
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

/// ルールもデフォルト変換も無い場合、URL はリンク化されずプレーンテキストとして
/// paragraph に表示される (link が null) ことを確認する。
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

/// bookmark のみのルールにマッチする URL は、先行テキストの paragraph と独立した
/// bookmark ブロック (インラインリンクを含まない) に分割されることを確認する。
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

/// link と bookmark の両方を指定したルールにマッチする URL は、インラインリンクを含む
/// paragraph と、それに続く bookmark ブロックの 2 つに変換されることを確認する。
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

/// embed のみのルールにマッチする URL 単体は、paragraph を生成せず embed ブロック
/// 1 つだけに変換されることを確認する。
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

/// 2000 文字を超える本文が、Notion の rich_text 上限に収まる複数の要素に
/// 分割されることを確認する。
///
/// Discord は Nitro で 4000 文字まで投稿できるため、分割しないと
/// append_blocks が 400 validation_error で失敗する。
#[test]
fn test_build_long_text_split_into_rich_text_chunks() {
    let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
    let text = "あ".repeat(4500);
    let result = build_rich_text_and_url_blocks(&text, &compiled);

    let mut restored = String::new();
    for (block_json, block_type) in &result.blocks {
        assert_eq!(block_type, "text");
        for rich_text in block_json["paragraph"]["rich_text"].as_array().unwrap() {
            let content = rich_text["text"]["content"].as_str().unwrap();
            assert!(
                content.chars().count() <= 2000,
                "rich_text 要素は 2000 文字以内であるべき: {}",
                content.chars().count()
            );
            restored.push_str(content);
        }
    }

    // 分割しても内容は失われない
    assert_eq!(restored, text);
}
