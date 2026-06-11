//! RelayWriteChannelMessage の単体テスト（共有ヘルパ）。

use crate::ports::{MockDiaryRepository, MockDiscordGateway};
use crate::test_support::{fixed_clock, utc};

use super::*;

mod edits;
mod relay;
mod worker;

fn settings() -> RelaySettings {
    RelaySettings {
        timezone: chrono_tz::UTC,
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
    sync: Arc<SyncDiaryMessage>,
) -> RelayWriteChannelMessage {
    RelayWriteChannelMessage::new(
        Arc::new(repo),
        Arc::new(gateway),
        Arc::new(fixed_clock(utc(2025, 1, 2, 9, 0))),
        sync,
        settings(),
    )
}
