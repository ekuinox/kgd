//! メッセージテキスト内の URL を解析し、Notion ブロック構築用のセグメントに分割する。

mod builder;
mod json;
mod matcher;

pub use builder::build_rich_text_and_url_blocks;
pub use json::apply_ogp_to_bookmark;
pub use matcher::{CompiledUrlRules, PatternConfig, UrlRuleConfig, compile_url_rules};

#[cfg(test)]
use builder::{TextSegment, classify_url, parse_segments};
#[cfg(test)]
use matcher::{UrlBlockType, UrlMatcher, UrlRule, parse_block_type};

#[cfg(test)]
mod tests;
