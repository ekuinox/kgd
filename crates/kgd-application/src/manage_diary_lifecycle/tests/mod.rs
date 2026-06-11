//! ManageDiaryLifecycle の単体テスト（共有ヘルパ）。

use crate::ports::{MockClock, MockDiaryRepository, MockDiscordGateway, MockNotionApi};

use super::*;

mod close;
mod create;

/// テスト用の設定を作る。
fn settings() -> DiaryLifecycleSettings {
    DiaryLifecycleSettings {
        timezone: chrono_tz::UTC,
        forum_channel_id: 555,
        write_channel_id: 500,
    }
}

fn lifecycle(
    repo: MockDiaryRepository,
    notion: MockNotionApi,
    gateway: MockDiscordGateway,
    clock: MockClock,
) -> ManageDiaryLifecycle {
    ManageDiaryLifecycle::new(
        Arc::new(repo),
        Arc::new(notion),
        Arc::new(gateway),
        Arc::new(clock),
        settings(),
    )
}
