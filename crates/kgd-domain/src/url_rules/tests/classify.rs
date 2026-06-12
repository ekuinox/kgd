//! classify_url のテスト。

use regex::Regex;

use super::*;

/// ルールもデフォルト変換も無い場合、URL の分類結果が空になることを確認する。
#[test]
fn test_classify_url_no_rules_no_default() {
    let compiled = compiled_with_rules(vec![]);
    assert!(classify_url("https://example.com", &compiled).is_empty());
}

/// ルールは無いがデフォルト変換 (Link) がある場合、どの URL もデフォルトの Link に
/// 分類されることを確認する。
#[test]
fn test_classify_url_no_rules_with_default() {
    let compiled = compiled_with_default(vec![], vec![UrlBlockType::Link]);
    assert_eq!(
        classify_url("https://example.com", &compiled),
        vec![UrlBlockType::Link]
    );
}

/// 正規表現ルールにマッチする URL が、そのルールの block_types (Bookmark) に
/// 分類されることを確認する。
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

/// どのルールにもマッチしない URL は、デフォルト変換 (Link) に分類されることを確認する。
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

/// 複数のルールが同じ URL にマッチしうる場合、先に定義されたルール (Embed) が
/// 優先採用されることを確認する。
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

/// glob マッチャのルールにマッチする URL が Bookmark に分類されることを確認する。
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

/// prefix マッチャのルールにマッチする URL が Bookmark に分類されることを確認する。
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
