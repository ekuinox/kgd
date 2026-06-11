//! RunDiaryMaintenance の単体テスト（共有ヘルパ）。

use crate::ports::{MockClock, MockDiaryRepository, MockDiscordGateway};

use super::*;

mod periodic;
mod thread_scan;

/// テスト用の設定を作る。
fn settings() -> DiaryMaintenanceSettings {
    DiaryMaintenanceSettings {
        timezone: chrono_tz::UTC,
        auto_close_enabled: true,
        auto_close_hour: 8,
        write_channel_id: 500,
        sync_reaction: "✅".to_string(),
    }
}

fn maintenance(
    repo: MockDiaryRepository,
    gateway: MockDiscordGateway,
    clock: MockClock,
    sync: Arc<SyncDiaryMessage>,
) -> RunDiaryMaintenance {
    RunDiaryMaintenance::new(
        Arc::new(repo),
        Arc::new(gateway),
        Arc::new(clock),
        sync,
        settings(),
    )
}
