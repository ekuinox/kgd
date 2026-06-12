//! 設定値のデフォルトを返す関数群。

use std::time::Duration;

use chrono_tz::Tz;

pub(super) fn default_interval() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

pub(super) fn default_title_property() -> String {
    "Name".to_string()
}

pub(super) fn default_sync_reaction() -> String {
    "✅".to_string()
}

pub(super) fn default_timezone() -> Tz {
    chrono_tz::Asia::Tokyo
}

pub(super) fn default_convert_to() -> Vec<String> {
    vec!["link".to_string()]
}

pub(super) fn default_auto_close_hour() -> u32 {
    8
}

pub(super) fn default_ogp_enabled() -> bool {
    true
}

pub(super) fn default_ogp_timeout() -> Duration {
    Duration::from_secs(10)
}
