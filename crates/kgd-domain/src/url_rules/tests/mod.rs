//! url_rules モジュールの単体テスト。

use super::*;

mod build;
mod build_mixed;
mod classify;
mod compile;
mod matcher;
mod ogp;
mod parse;

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
