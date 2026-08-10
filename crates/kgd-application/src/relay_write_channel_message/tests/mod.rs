//! RelayWriteChannelMessageUseCase の単体テスト（共有ヘルパ）。

use crate::ports::{MockDiaryRepository, MockDiscordGateway};
use crate::test_support::{fixed_clock, utc};

use super::*;

mod edits;
mod relay;
mod worker;

fn settings() -> RelaySettings {
    settings_with(DiaryCalendar::new(chrono_tz::UTC, 0))
}

fn settings_with(calendar: DiaryCalendar) -> RelaySettings {
    RelaySettings {
        calendar,
        sync_reaction: "✅".to_string(),
    }
}

fn message(message_id: u64, content: &str) -> SyncMessage {
    SyncMessage {
        message_id,
        channel_id: 500,
        guild_id: Some(1),
        content: content.to_string(),
        is_bot: false,
        attachments: vec![],
    }
}

fn relay_use_case(
    repo: MockDiaryRepository,
    gateway: MockDiscordGateway,
    sync: Arc<SyncDiaryMessageUseCase>,
) -> RelayWriteChannelMessageUseCase {
    RelayWriteChannelMessageUseCase::new(
        Arc::new(repo),
        Arc::new(gateway),
        Arc::new(fixed_clock(utc(2025, 1, 2, 9, 0))),
        sync,
        settings(),
    )
}

/// 時刻とカレンダーを指定して転記ユースケースを作る。
fn relay_use_case_at(
    now: DateTime<Utc>,
    calendar: DiaryCalendar,
    repo: MockDiaryRepository,
    gateway: MockDiscordGateway,
    sync: Arc<SyncDiaryMessageUseCase>,
) -> RelayWriteChannelMessageUseCase {
    RelayWriteChannelMessageUseCase::new(
        Arc::new(repo),
        Arc::new(gateway),
        Arc::new(fixed_clock(now)),
        sync,
        settings_with(calendar),
    )
}
