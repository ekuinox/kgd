//! compile_url_rules と expect バリデーションのテスト。

use super::*;

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
