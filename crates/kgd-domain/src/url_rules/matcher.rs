//! URL 変換ルールの設定とコンパイル・マッチング。

use anyhow::{Result, bail};
use regex::Regex;

use serde::{Deserialize, Serialize};

/// URL 変換ルール設定。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct UrlRuleConfig {
    /// マッチする URL パターン
    pub pattern: PatternConfig,
    /// 生成するブロックタイプのリスト（link, bookmark, embed）
    pub convert_to: Vec<String>,
    /// このパターンにマッチすべき URL の一覧（起動時バリデーション用）
    #[serde(default)]
    pub expect_matches: Vec<String>,
    /// このパターンにマッチすべきでない URL の一覧（起動時バリデーション用）
    #[serde(default)]
    pub expect_no_matches: Vec<String>,
}

/// URL マッチパターンの種類。
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternConfig {
    /// glob 形式のパターン
    Glob(String),
    /// 正規表現パターン
    Regex(String),
    /// 前方一致パターン
    Prefix(String),
}

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
pub(crate) enum UrlMatcher {
    /// glob パターンでマッチ
    Glob(String),
    /// 正規表現でマッチ
    Regex(Regex),
    /// 前方一致でマッチ
    Prefix(String),
}

impl UrlMatcher {
    /// URL がパターンにマッチするかを判定する。
    pub(crate) fn is_match(&self, url: &str) -> bool {
        match self {
            UrlMatcher::Glob(pattern) => glob_match::glob_match(pattern, url),
            UrlMatcher::Regex(re) => re.is_match(url),
            UrlMatcher::Prefix(prefix) => url.starts_with(prefix.as_str()),
        }
    }
}

/// コンパイル済み URL 変換ルール。
pub(crate) struct UrlRule {
    /// マッチする URL パターン
    pub(crate) matcher: UrlMatcher,
    /// 生成するブロックタイプのリスト
    pub(crate) block_types: Vec<UrlBlockType>,
}

/// コンパイル済み URL 変換ルール一式。
pub struct CompiledUrlRules {
    /// パターンごとのルール
    pub(crate) rules: Vec<UrlRule>,
    /// どのルールにもマッチしなかった URL に適用するデフォルトの変換
    pub(crate) default_types: Vec<UrlBlockType>,
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

/// ブロックタイプ文字列をパースする。
pub(crate) fn parse_block_type(s: &str) -> Option<UrlBlockType> {
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
