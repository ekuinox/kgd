//! UrlMatcher と parse_block_type のテスト。

use regex::Regex;

use super::*;

/// glob マッチャが、パターンに合致する URL では true、合致しない URL では false を
/// 返すことを確認する。
#[test]
fn test_url_matcher_glob() {
    let matcher = UrlMatcher::Glob("https://youtube.com/watch?v=*".to_string());
    assert!(matcher.is_match("https://youtube.com/watch?v=abc123"));
    assert!(!matcher.is_match("https://youtube.com/playlist?list=abc"));
}

/// prefix マッチャが、指定プレフィックスで始まる URL では true、そうでない URL では
/// false を返すことを確認する。
#[test]
fn test_url_matcher_prefix() {
    let matcher = UrlMatcher::Prefix("https://github.com/".to_string());
    assert!(matcher.is_match("https://github.com/ekuinox/kgd"));
    assert!(!matcher.is_match("https://gitlab.com/user/repo"));
}

/// regex マッチャが、正規表現に合致する URL では true、合致しない URL では false を
/// 返すことを確認する。
#[test]
fn test_url_matcher_regex() {
    let matcher = UrlMatcher::Regex(Regex::new(r"https://twitter\.com/.+/status/\d+").unwrap());
    assert!(matcher.is_match("https://twitter.com/user/status/123"));
    assert!(!matcher.is_match("https://twitter.com/user"));
}

/// parse_block_type が "link"/"bookmark"/"embed" を各 UrlBlockType に変換し、
/// 未知の文字列には None を返すことを確認する。
#[test]
fn test_parse_block_type_all_variants() {
    assert_eq!(parse_block_type("link"), Some(UrlBlockType::Link));
    assert_eq!(parse_block_type("bookmark"), Some(UrlBlockType::Bookmark));
    assert_eq!(parse_block_type("embed"), Some(UrlBlockType::Embed));
    assert_eq!(parse_block_type("unknown"), None);
}
