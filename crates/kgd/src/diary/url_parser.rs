//! メッセージテキスト内の URL を解析し、Notion ブロック構築用のセグメントに分割する。

use anyhow::{Result, bail};
use regex::Regex;

use crate::config::{PatternConfig, UrlRuleConfig};

/// URL から生成する変換の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlBlockType {
    /// rich_text 内のインラインリンク
    Link,
    /// Notion ブックマークブロック
    Bookmark,
    /// Notion 埋め込みブロック
    Embed,
}

/// URL マッチング方法。
enum UrlMatcher {
    /// glob パターンでマッチ
    Glob(String),
    /// 正規表現でマッチ
    Regex(Regex),
    /// 前方一致でマッチ
    Prefix(String),
}

impl UrlMatcher {
    /// URL がパターンにマッチするかを判定する。
    fn is_match(&self, url: &str) -> bool {
        match self {
            UrlMatcher::Glob(pattern) => glob_match::glob_match(pattern, url),
            UrlMatcher::Regex(re) => re.is_match(url),
            UrlMatcher::Prefix(prefix) => url.starts_with(prefix.as_str()),
        }
    }
}

/// コンパイル済み URL 変換ルール。
struct UrlRule {
    /// マッチする URL パターン
    matcher: UrlMatcher,
    /// 生成するブロックタイプのリスト
    block_types: Vec<UrlBlockType>,
}

/// コンパイル済み URL 変換ルール一式。
pub struct CompiledUrlRules {
    /// パターンごとのルール
    rules: Vec<UrlRule>,
    /// どのルールにもマッチしなかった URL に適用するデフォルトの変換
    default_types: Vec<UrlBlockType>,
}

/// URL 解析結果のブロック。出現順に並ぶ。
pub struct UrlParseResult {
    /// 出現順の Notion ブロック JSON と block_type 文字列のペア
    pub blocks: Vec<(serde_json::Value, String)>,
}

/// 設定からコンパイル済み URL ルールを作成する。
///
/// 各ルールの `expect_matches` / `expect_no_matches` によるバリデーションも行い、
/// 期待通りでない場合はエラーを返す。
/// 無効なパターンや不明なブロックタイプはエラーとして返す。
pub fn compile_url_rules(
    rules: &[UrlRuleConfig],
    default_convert_to: &[String],
) -> Result<CompiledUrlRules> {
    let mut compiled_rules = Vec::new();

    for rule in rules {
        let matcher = match &rule.pattern {
            PatternConfig::Glob(pattern) => UrlMatcher::Glob(pattern.clone()),
            PatternConfig::Regex(pattern) => {
                let re = Regex::new(pattern)
                    .map_err(|e| anyhow::anyhow!("Invalid regex pattern '{}': {}", pattern, e))?;
                UrlMatcher::Regex(re)
            }
            PatternConfig::Prefix(prefix) => UrlMatcher::Prefix(prefix.clone()),
        };

        let block_types: Vec<UrlBlockType> = rule
            .convert_to
            .iter()
            .filter_map(|s| parse_block_type(s))
            .collect();

        if block_types.is_empty() {
            bail!(
                "No valid block types in convert_to for pattern {:?}",
                rule.pattern
            );
        }

        // expect_matches のバリデーション
        for url in &rule.expect_matches {
            if !matcher.is_match(url) {
                bail!(
                    "URL pattern {:?} expected to match '{}' but did not",
                    rule.pattern,
                    url
                );
            }
        }

        // expect_no_matches のバリデーション
        for url in &rule.expect_no_matches {
            if matcher.is_match(url) {
                bail!(
                    "URL pattern {:?} expected NOT to match '{}' but it did",
                    rule.pattern,
                    url
                );
            }
        }

        compiled_rules.push(UrlRule {
            matcher,
            block_types,
        });
    }

    let default_types = default_convert_to
        .iter()
        .filter_map(|s| parse_block_type(s))
        .collect();

    Ok(CompiledUrlRules {
        rules: compiled_rules,
        default_types,
    })
}

/// テキストからセグメントを解析し、出現順に Notion ブロックを生成する。
///
/// テキストやインラインリンクは paragraph ブロックにまとめ、
/// bookmark/embed が出現する位置で paragraph を分割して順序を保持する。
pub fn build_rich_text_and_url_blocks(text: &str, compiled: &CompiledUrlRules) -> UrlParseResult {
    let segments = parse_segments(text);
    let mut blocks: Vec<(serde_json::Value, String)> = Vec::new();
    let mut pending_rich_text: Vec<serde_json::Value> = Vec::new();

    for segment in segments {
        match segment {
            TextSegment::Plain(s) => {
                if !s.is_empty() {
                    pending_rich_text.push(serde_json::json!({
                        "type": "text",
                        "text": {
                            "content": s
                        }
                    }));
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
                            blocks.push((bookmark_block_json(&url), "bookmark".to_string()));
                        }
                        UrlBlockType::Embed => {
                            blocks.push((embed_block_json(&url), "embed".to_string()));
                        }
                    }
                }

                // いずれの変換も行われない場合のみプレーンテキストとして URL を表示
                if block_types.is_empty() {
                    pending_rich_text.push(plain_text_json(&url));
                }
            }
        }
    }

    // 残りの rich_text を paragraph として追加
    flush_paragraph(&mut pending_rich_text, &mut blocks);

    UrlParseResult { blocks }
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
enum TextSegment {
    /// 通常のテキスト
    Plain(String),
    /// URL
    Url(String),
}

/// テキストを URL とプレーンテキストのセグメントに分割する。
fn parse_segments(text: &str) -> Vec<TextSegment> {
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
fn classify_url(url: &str, compiled: &CompiledUrlRules) -> Vec<UrlBlockType> {
    for rule in &compiled.rules {
        if rule.matcher.is_match(url) {
            return rule.block_types.clone();
        }
    }
    compiled.default_types.clone()
}

/// ブロックタイプ文字列をパースする。
fn parse_block_type(s: &str) -> Option<UrlBlockType> {
    match s {
        "link" => Some(UrlBlockType::Link),
        "bookmark" => Some(UrlBlockType::Bookmark),
        "embed" => Some(UrlBlockType::Embed),
        _ => {
            tracing::warn!(block_type = %s, "Unknown block type in convert_to, skipping");
            None
        }
    }
}

/// プレーンテキストの rich_text JSON を生成する。
fn plain_text_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "text",
        "text": {
            "content": text
        }
    })
}

/// インラインリンクの rich_text JSON を生成する。
fn inline_link_json(url: &str) -> serde_json::Value {
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
fn bookmark_block_json(url: &str) -> serde_json::Value {
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
fn embed_block_json(url: &str) -> serde_json::Value {
    serde_json::json!({
        "object": "block",
        "type": "embed",
        "embed": {
            "url": url
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_segments_no_urls() {
        let result = parse_segments("hello world");
        assert_eq!(result, vec![TextSegment::Plain("hello world".to_string())]);
    }

    #[test]
    fn test_parse_segments_single_url() {
        let result = parse_segments("check https://example.com please");
        assert_eq!(
            result,
            vec![
                TextSegment::Plain("check ".to_string()),
                TextSegment::Url("https://example.com".to_string()),
                TextSegment::Plain(" please".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_segments_multiple_urls() {
        let result = parse_segments("https://a.com and https://b.com");
        assert_eq!(
            result,
            vec![
                TextSegment::Url("https://a.com".to_string()),
                TextSegment::Plain(" and ".to_string()),
                TextSegment::Url("https://b.com".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_segments_url_only() {
        let result = parse_segments("https://example.com");
        assert_eq!(
            result,
            vec![TextSegment::Url("https://example.com".to_string())]
        );
    }

    #[test]
    fn test_parse_segments_http_url() {
        let result = parse_segments("link: http://example.com");
        assert_eq!(
            result,
            vec![
                TextSegment::Plain("link: ".to_string()),
                TextSegment::Url("http://example.com".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_segments_empty() {
        let result = parse_segments("");
        assert!(result.is_empty());
    }

    /// デフォルト変換なしの CompiledUrlRules を作成するヘルパー。
    fn compiled_with_rules(rules: Vec<UrlRule>) -> CompiledUrlRules {
        CompiledUrlRules {
            rules,
            default_types: vec![],
        }
    }

    /// デフォルト変換ありの CompiledUrlRules を作成するヘルパー。
    fn compiled_with_default(
        rules: Vec<UrlRule>,
        default_types: Vec<UrlBlockType>,
    ) -> CompiledUrlRules {
        CompiledUrlRules {
            rules,
            default_types,
        }
    }

    #[test]
    fn test_classify_url_no_rules_no_default() {
        let compiled = compiled_with_rules(vec![]);
        assert!(classify_url("https://example.com", &compiled).is_empty());
    }

    #[test]
    fn test_classify_url_no_rules_with_default() {
        let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
        assert_eq!(
            classify_url("https://example.com", &compiled),
            vec![UrlBlockType::Link]
        );
    }

    #[test]
    fn test_classify_url_matching_rule() {
        let compiled = compiled_with_default(
            vec![UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
                block_types: vec![UrlBlockType::Bookmark],
            }],
            vec![UrlBlockType::Link],
        );
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &compiled),
            vec![UrlBlockType::Bookmark]
        );
    }

    #[test]
    fn test_classify_url_non_matching_rule_uses_default() {
        let compiled = compiled_with_default(
            vec![UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
                block_types: vec![UrlBlockType::Bookmark],
            }],
            vec![UrlBlockType::Link],
        );
        assert_eq!(
            classify_url("https://example.com", &compiled),
            vec![UrlBlockType::Link]
        );
    }

    #[test]
    fn test_classify_url_first_match_wins() {
        let compiled = compiled_with_rules(vec![
            UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
                block_types: vec![UrlBlockType::Embed],
            },
            UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://.*").unwrap()),
                block_types: vec![UrlBlockType::Bookmark],
            },
        ]);
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &compiled),
            vec![UrlBlockType::Embed]
        );
    }

    #[test]
    fn test_classify_url_glob_matching() {
        let compiled = compiled_with_rules(vec![UrlRule {
            matcher: UrlMatcher::Glob("https://github.com/**".to_string()),
            block_types: vec![UrlBlockType::Bookmark],
        }]);
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &compiled),
            vec![UrlBlockType::Bookmark]
        );
    }

    #[test]
    fn test_classify_url_prefix_matching() {
        let compiled = compiled_with_rules(vec![UrlRule {
            matcher: UrlMatcher::Prefix("https://github.com/".to_string()),
            block_types: vec![UrlBlockType::Bookmark],
        }]);
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &compiled),
            vec![UrlBlockType::Bookmark]
        );
    }

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
        let result =
            build_rich_text_and_url_blocks("check https://github.com/ekuinox/kgd", &compiled);
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
        let result =
            build_rich_text_and_url_blocks("check https://github.com/ekuinox/kgd", &compiled);
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
        let result =
            build_rich_text_and_url_blocks("before https://github.com/foo after", &compiled);
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
    fn test_compile_url_rules_regex_valid() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://github\.com/.*".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        let compiled = compile_url_rules(&rules, &["link".to_string()]).unwrap();
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(compiled.rules[0].block_types, vec![UrlBlockType::Bookmark]);
        assert_eq!(compiled.default_types, vec![UrlBlockType::Link]);
    }

    #[test]
    fn test_compile_url_rules_invalid_regex() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex("[invalid".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        assert!(compile_url_rules(&rules, &[]).is_err());
    }

    #[test]
    fn test_compile_url_rules_glob() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Glob("https://github.com/**".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        let compiled = compile_url_rules(&rules, &[]).unwrap();
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(compiled.rules[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_prefix() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Prefix("https://github.com/".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        let compiled = compile_url_rules(&rules, &[]).unwrap();
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(compiled.rules[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_unknown_block_type() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://example\.com/.*".to_string()),
            convert_to: vec!["unknown_type".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        // 有効なブロックタイプがないのでエラー
        assert!(compile_url_rules(&rules, &[]).is_err());
    }

    #[test]
    fn test_compile_url_rules_partial_valid_block_types() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://example\.com/.*".to_string()),
            convert_to: vec!["bookmark".to_string(), "invalid".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        let compiled = compile_url_rules(&rules, &[]).unwrap();
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(compiled.rules[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_empty() {
        let compiled = compile_url_rules(&[], &[]).unwrap();
        assert!(compiled.rules.is_empty());
    }

    #[test]
    fn test_compile_url_rules_with_link_type() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Prefix("https://github.com/".to_string()),
            convert_to: vec!["link".to_string(), "bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec![],
        }];
        let compiled = compile_url_rules(&rules, &["link".to_string()]).unwrap();
        assert_eq!(compiled.rules.len(), 1);
        assert_eq!(
            compiled.rules[0].block_types,
            vec![UrlBlockType::Link, UrlBlockType::Bookmark]
        );
    }

    #[test]
    fn test_compile_url_rules_expect_matches_pass() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Glob("https://www.youtube.com/watch*".to_string()),
            convert_to: vec!["embed".to_string(), "bookmark".to_string()],
            expect_matches: vec!["https://www.youtube.com/watch?v=DFaYoGSCKbs".to_string()],
            expect_no_matches: vec!["https://www.youtube.com/".to_string()],
        }];
        assert!(compile_url_rules(&rules, &[]).is_ok());
    }

    #[test]
    fn test_compile_url_rules_expect_matches_fail() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Prefix("https://github.com/".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec!["https://gitlab.com/user/repo".to_string()],
            expect_no_matches: vec![],
        }];
        assert!(compile_url_rules(&rules, &[]).is_err());
    }

    #[test]
    fn test_compile_url_rules_expect_no_matches_fail() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Prefix("https://github.com/".to_string()),
            convert_to: vec!["bookmark".to_string()],
            expect_matches: vec![],
            expect_no_matches: vec!["https://github.com/ekuinox/kgd".to_string()],
        }];
        assert!(compile_url_rules(&rules, &[]).is_err());
    }

    #[test]
    fn test_url_matcher_glob() {
        let matcher = UrlMatcher::Glob("https://youtube.com/watch?v=*".to_string());
        assert!(matcher.is_match("https://youtube.com/watch?v=abc123"));
        assert!(!matcher.is_match("https://youtube.com/playlist?list=abc"));
    }

    #[test]
    fn test_url_matcher_prefix() {
        let matcher = UrlMatcher::Prefix("https://github.com/".to_string());
        assert!(matcher.is_match("https://github.com/ekuinox/kgd"));
        assert!(!matcher.is_match("https://gitlab.com/user/repo"));
    }

    #[test]
    fn test_url_matcher_regex() {
        let matcher = UrlMatcher::Regex(Regex::new(r"https://twitter\.com/.+/status/\d+").unwrap());
        assert!(matcher.is_match("https://twitter.com/user/status/123"));
        assert!(!matcher.is_match("https://twitter.com/user"));
    }

    #[test]
    fn test_parse_block_type_all_variants() {
        assert_eq!(parse_block_type("link"), Some(UrlBlockType::Link));
        assert_eq!(parse_block_type("bookmark"), Some(UrlBlockType::Bookmark));
        assert_eq!(parse_block_type("embed"), Some(UrlBlockType::Embed));
        assert_eq!(parse_block_type("unknown"), None);
    }
}
