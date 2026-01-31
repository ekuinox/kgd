//! メッセージテキスト内の URL を解析し、Notion ブロック構築用のセグメントに分割する。

use regex::Regex;

use crate::config::{PatternConfig, UrlRuleConfig};

/// URL から生成するブロックの種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlBlockType {
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
pub struct UrlRule {
    /// マッチする URL パターン
    matcher: UrlMatcher,
    /// 生成するブロックタイプのリスト
    block_types: Vec<UrlBlockType>,
}

/// URL 解析結果。
pub struct UrlParseResult {
    /// paragraph ブロックの rich_text に使用する JSON 配列
    pub rich_text: Vec<serde_json::Value>,
    /// テキストの後に追加するブロック JSON 配列と block_type 文字列のペア
    pub extra_blocks: Vec<(serde_json::Value, String)>,
}

/// 設定からコンパイル済み URL ルールを作成する。
///
/// 無効なパターンや不明なブロックタイプは警告をログに出力してスキップする。
pub fn compile_url_rules(rules: &[UrlRuleConfig]) -> Vec<UrlRule> {
    rules
        .iter()
        .filter_map(|rule| {
            let matcher = match &rule.pattern {
                PatternConfig::Glob(pattern) => UrlMatcher::Glob(pattern.clone()),
                PatternConfig::Regex(pattern) => match Regex::new(pattern) {
                    Ok(re) => UrlMatcher::Regex(re),
                    Err(e) => {
                        tracing::warn!(pattern = %pattern, error = %e, "Invalid regex pattern, skipping rule");
                        return None;
                    }
                },
                PatternConfig::Prefix(prefix) => UrlMatcher::Prefix(prefix.clone()),
            };

            let block_types: Vec<UrlBlockType> = rule
                .convert_to
                .iter()
                .filter_map(|s| parse_block_type(s))
                .collect();

            if block_types.is_empty() {
                tracing::warn!("No valid block types in convert_to, skipping rule");
                return None;
            }

            Some(UrlRule {
                matcher,
                block_types,
            })
        })
        .collect()
}

/// テキストからセグメントを解析し、Notion 用の rich_text JSON 配列と
/// 追加ブロックを生成する。
pub fn build_rich_text_and_url_blocks(text: &str, rules: &[UrlRule]) -> UrlParseResult {
    let segments = parse_segments(text);
    let mut rich_text: Vec<serde_json::Value> = Vec::new();
    let mut extra_blocks: Vec<(serde_json::Value, String)> = Vec::new();

    for segment in segments {
        match segment {
            TextSegment::Plain(s) => {
                if !s.is_empty() {
                    rich_text.push(serde_json::json!({
                        "type": "text",
                        "text": {
                            "content": s
                        }
                    }));
                }
            }
            TextSegment::Url(url) => {
                let matched_types = classify_url(&url, rules);

                // URL は常にインラインリンクとして rich_text に含める
                rich_text.push(inline_link_json(&url));

                // ルールにマッチした場合は追加ブロックを生成
                for block_type in &matched_types {
                    match block_type {
                        UrlBlockType::Bookmark => {
                            extra_blocks.push((bookmark_block_json(&url), "bookmark".to_string()));
                        }
                        UrlBlockType::Embed => {
                            extra_blocks.push((embed_block_json(&url), "embed".to_string()));
                        }
                    }
                }
            }
        }
    }

    UrlParseResult {
        rich_text,
        extra_blocks,
    }
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

/// URL にマッチするルールのブロックタイプ一覧を返す。最初にマッチしたルールのみ適用。
fn classify_url(url: &str, rules: &[UrlRule]) -> Vec<UrlBlockType> {
    for rule in rules {
        if rule.matcher.is_match(url) {
            return rule.block_types.clone();
        }
    }
    vec![]
}

/// ブロックタイプ文字列をパースする。
fn parse_block_type(s: &str) -> Option<UrlBlockType> {
    match s {
        "bookmark" => Some(UrlBlockType::Bookmark),
        "embed" => Some(UrlBlockType::Embed),
        _ => {
            tracing::warn!(block_type = %s, "Unknown block type in convert_to, skipping");
            None
        }
    }
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

    #[test]
    fn test_classify_url_no_rules() {
        assert!(classify_url("https://example.com", &[]).is_empty());
    }

    #[test]
    fn test_classify_url_matching_rule() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &rules),
            vec![UrlBlockType::Bookmark]
        );
    }

    #[test]
    fn test_classify_url_non_matching_rule() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        assert!(classify_url("https://example.com", &rules).is_empty());
    }

    #[test]
    fn test_classify_url_first_match_wins() {
        let rules = vec![
            UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
                block_types: vec![UrlBlockType::Embed],
            },
            UrlRule {
                matcher: UrlMatcher::Regex(Regex::new(r"https://.*").unwrap()),
                block_types: vec![UrlBlockType::Bookmark],
            },
        ];
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &rules),
            vec![UrlBlockType::Embed]
        );
    }

    #[test]
    fn test_classify_url_glob_matching() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Glob("https://github.com/**".to_string()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &rules),
            vec![UrlBlockType::Bookmark]
        );
        assert!(classify_url("https://example.com", &rules).is_empty());
    }

    #[test]
    fn test_classify_url_prefix_matching() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Prefix("https://github.com/".to_string()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        assert_eq!(
            classify_url("https://github.com/ekuinox/kgd", &rules),
            vec![UrlBlockType::Bookmark]
        );
        assert!(classify_url("https://example.com", &rules).is_empty());
    }

    #[test]
    fn test_build_no_urls() {
        let result = build_rich_text_and_url_blocks("plain text", &[]);
        assert_eq!(result.rich_text.len(), 1);
        assert_eq!(result.rich_text[0]["text"]["content"], "plain text");
        assert!(result.rich_text[0]["text"]["link"].is_null());
        assert!(result.extra_blocks.is_empty());
    }

    #[test]
    fn test_build_inline_link_no_rules() {
        let result = build_rich_text_and_url_blocks("see https://example.com here", &[]);
        assert_eq!(result.rich_text.len(), 3);
        assert_eq!(
            result.rich_text[1]["text"]["content"],
            "https://example.com"
        );
        assert_eq!(
            result.rich_text[1]["text"]["link"]["url"],
            "https://example.com"
        );
        assert!(result.extra_blocks.is_empty());
    }

    #[test]
    fn test_build_bookmark_rule() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        let result = build_rich_text_and_url_blocks("check https://github.com/ekuinox/kgd", &rules);
        assert_eq!(result.rich_text.len(), 2);
        // URL はインラインリンクとして含まれる（mention ではないため）
        assert_eq!(
            result.rich_text[1]["text"]["link"]["url"],
            "https://github.com/ekuinox/kgd"
        );
        assert_eq!(result.extra_blocks.len(), 1);
        assert_eq!(result.extra_blocks[0].1, "bookmark");
        assert_eq!(
            result.extra_blocks[0].0["bookmark"]["url"],
            "https://github.com/ekuinox/kgd"
        );
    }

    #[test]
    fn test_build_embed_rule() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://youtube\.com/watch.*").unwrap()),
            block_types: vec![UrlBlockType::Embed],
        }];
        let result = build_rich_text_and_url_blocks("https://youtube.com/watch?v=abc", &rules);
        assert_eq!(result.extra_blocks.len(), 1);
        assert_eq!(result.extra_blocks[0].1, "embed");
        assert_eq!(
            result.extra_blocks[0].0["embed"]["url"],
            "https://youtube.com/watch?v=abc"
        );
    }

    #[test]
    fn test_build_multiple_block_types() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://youtube\.com/watch.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark, UrlBlockType::Embed],
        }];
        let result = build_rich_text_and_url_blocks("https://youtube.com/watch?v=abc", &rules);
        assert_eq!(result.extra_blocks.len(), 2);
        assert_eq!(result.extra_blocks[0].1, "bookmark");
        assert_eq!(result.extra_blocks[1].1, "embed");
    }

    #[test]
    fn test_build_mixed_urls() {
        let rules = vec![UrlRule {
            matcher: UrlMatcher::Regex(Regex::new(r"https://github\.com/.*").unwrap()),
            block_types: vec![UrlBlockType::Bookmark],
        }];
        let result = build_rich_text_and_url_blocks(
            "see https://example.com and https://github.com/ekuinox/kgd",
            &rules,
        );
        assert_eq!(result.rich_text.len(), 4);
        // example.com はインラインリンク
        assert_eq!(result.rich_text[1]["type"], "text");
        assert_eq!(
            result.rich_text[1]["text"]["link"]["url"],
            "https://example.com"
        );
        // github.com もインラインリンク + bookmark ブロック
        assert_eq!(result.rich_text[3]["type"], "text");
        assert_eq!(
            result.rich_text[3]["text"]["link"]["url"],
            "https://github.com/ekuinox/kgd"
        );
        assert_eq!(result.extra_blocks.len(), 1);
        assert_eq!(result.extra_blocks[0].1, "bookmark");
    }

    #[test]
    fn test_compile_url_rules_regex_valid() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://github\.com/.*".to_string()),
            convert_to: vec!["bookmark".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_invalid_regex() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex("[invalid".to_string()),
            convert_to: vec!["bookmark".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        assert!(compiled.is_empty());
    }

    #[test]
    fn test_compile_url_rules_glob() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Glob("https://github.com/**".to_string()),
            convert_to: vec!["bookmark".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_prefix() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Prefix("https://github.com/".to_string()),
            convert_to: vec!["bookmark".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_unknown_block_type() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://example\.com/.*".to_string()),
            convert_to: vec!["unknown_type".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        // 有効なブロックタイプがないのでルールごとスキップ
        assert!(compiled.is_empty());
    }

    #[test]
    fn test_compile_url_rules_partial_valid_block_types() {
        let rules = vec![UrlRuleConfig {
            pattern: PatternConfig::Regex(r"https://example\.com/.*".to_string()),
            convert_to: vec!["bookmark".to_string(), "invalid".to_string()],
        }];
        let compiled = compile_url_rules(&rules);
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].block_types, vec![UrlBlockType::Bookmark]);
    }

    #[test]
    fn test_compile_url_rules_empty() {
        let compiled = compile_url_rules(&[]);
        assert!(compiled.is_empty());
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
        assert_eq!(parse_block_type("bookmark"), Some(UrlBlockType::Bookmark));
        assert_eq!(parse_block_type("embed"), Some(UrlBlockType::Embed));
        assert_eq!(parse_block_type("mention"), None);
        assert_eq!(parse_block_type("link_preview"), None);
        assert_eq!(parse_block_type("unknown"), None);
    }
}
