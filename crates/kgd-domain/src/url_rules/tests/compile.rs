//! compile_url_rules と expect バリデーションのテスト。

use super::*;

/// 有効な正規表現パターンと bookmark 変換のルールをコンパイルし、ルールが 1 件・
/// block_types が Bookmark・デフォルトが Link として正しく反映されることを確認する。
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

/// 不正な正規表現パターン (`[invalid`) を含むルールのコンパイルがエラーになることを確認する。
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

/// glob パターンと bookmark 変換のルールが正常にコンパイルされ、
/// block_types が Bookmark になることを確認する。
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

/// prefix パターンと bookmark 変換のルールが正常にコンパイルされ、
/// block_types が Bookmark になることを確認する。
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

/// convert_to が未知のブロックタイプのみで有効な変換先が 1 つも無い場合、
/// コンパイルがエラーになることを確認する。
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

/// convert_to に有効 (bookmark) と無効 (invalid) のタイプが混在する場合、
/// 無効なタイプは無視され有効な Bookmark のみが block_types に残ることを確認する。
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

/// ルールもデフォルトも空の場合、コンパイル結果のルールが空になることを確認する。
#[test]
fn test_compile_url_rules_empty() {
    let compiled = compile_url_rules(&[], &[]).unwrap();
    assert!(compiled.rules.is_empty());
}

/// convert_to に link と bookmark を指定した場合、block_types が指定順 (Link, Bookmark)
/// で保持されることを確認する。
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

/// expect_matches の URL がマッチし expect_no_matches の URL がマッチしない場合、
/// バリデーションを通過してコンパイルが成功することを確認する。
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

/// expect_matches に指定した URL が実際にはパターンにマッチしない場合、
/// バリデーションに失敗してコンパイルがエラーになることを確認する。
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

/// expect_no_matches に指定した URL が実際にはパターンにマッチしてしまう場合、
/// バリデーションに失敗してコンパイルがエラーになることを確認する。
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
