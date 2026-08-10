//! RunDiaryMaintenanceUseCase の単体テスト（共有ヘルパ）。

use kgd_domain::DiaryCalendar;

use crate::ports::{MockClock, MockDiaryRepository, MockDiscordGateway};

use super::*;

mod periodic;
mod thread_scan;

/// テスト用の設定を作る。
fn settings() -> DiaryMaintenanceSettings {
    DiaryMaintenanceSettings {
        calendar: DiaryCalendar::new(chrono_tz::UTC, 8),
        auto_close_enabled: true,
        write_channel_id: 500,
        sync_reaction: "✅".to_string(),
    }
}

fn maintenance(
    repo: MockDiaryRepository,
    gateway: MockDiscordGateway,
    clock: MockClock,
    sync: Arc<SyncDiaryMessageUseCase>,
) -> RunDiaryMaintenanceUseCase {
    RunDiaryMaintenanceUseCase::new(
        Arc::new(repo),
        Arc::new(gateway),
        Arc::new(clock),
        sync,
        settings(),
    )
}
