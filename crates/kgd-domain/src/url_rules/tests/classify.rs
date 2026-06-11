//! classify_url のテスト。

use regex::Regex;

use super::*;

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
