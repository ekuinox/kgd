use const_format::formatcp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = env!("VERGEN_GIT_SHA");
pub const BUILD_DATE: &str = env!("VERGEN_BUILD_DATE");
pub const TARGET_TRIPLE: &str = env!("VERGEN_CARGO_TARGET_TRIPLE");

/// clap の `--version` 用のバージョン文字列を返す。
pub fn short_version() -> &'static str {
    formatcp!("{VERSION} ({GIT_SHA} {BUILD_DATE})")
}
