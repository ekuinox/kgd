//! 設定値のデフォルトを返す関数群。

use std::{net::SocketAddr, path::PathBuf, time::Duration};

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

pub(super) fn default_day_start_hour() -> u32 {
    8
}

pub(super) fn default_ogp_enabled() -> bool {
    true
}

pub(super) fn default_ogp_timeout() -> Duration {
    Duration::from_secs(10)
}

pub(super) fn default_location_listen() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 8081))
}

pub(super) fn default_daily_report_enabled() -> bool {
    true
}

pub(super) fn default_max_accuracy_m() -> i32 {
    200
}

pub(super) fn default_image_size() -> u32 {
    1024
}

pub(super) fn default_tile_cache_dir() -> PathBuf {
    PathBuf::from("/var/cache/kgd/tiles")
}
