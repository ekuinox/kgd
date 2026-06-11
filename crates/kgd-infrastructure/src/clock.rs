//! システム時刻を返す Clock ポートの実装。

use chrono::{DateTime, Utc};

use kgd_application::ports::Clock;

/// システム時刻をそのまま返す [`Clock`] 実装。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
