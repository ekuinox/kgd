//! ManageDiaryLifecycleUseCase の単体テスト（共有ヘルパ）。

use kgd_domain::DiaryCalendar;

use crate::ports::{MockClock, MockDiaryRepository, MockDiscordGateway, MockNotionApi};

use super::*;

mod close;
mod create;

/// テスト用の設定を作る。
fn settings() -> DiaryLifecycleSettings {
    settings_with(DiaryCalendar::new(chrono_tz::UTC, 0))
}

fn settings_with(calendar: DiaryCalendar) -> DiaryLifecycleSettings {
    DiaryLifecycleSettings {
        calendar,
        forum_channel_id: 555,
        write_channel_id: 500,
    }
}

fn lifecycle(
    repo: MockDiaryRepository,
    notion: MockNotionApi,
    gateway: MockDiscordGateway,
    clock: MockClock,
) -> ManageDiaryLifecycleUseCase {
    ManageDiaryLifecycleUseCase::new(
        Arc::new(repo),
        Arc::new(notion),
        Arc::new(gateway),
        Arc::new(clock),
        settings(),
    )
}

/// カレンダーを指定してライフサイクルユースケースを作る。
fn lifecycle_with(
    calendar: DiaryCalendar,
    repo: MockDiaryRepository,
    notion: MockNotionApi,
    gateway: MockDiscordGateway,
    clock: MockClock,
) -> ManageDiaryLifecycleUseCase {
    ManageDiaryLifecycleUseCase::new(
        Arc::new(repo),
        Arc::new(notion),
        Arc::new(gateway),
        Arc::new(clock),
        settings_with(calendar),
    )
}
