//! ユースケースのテスト間で共有するヘルパー。

use std::sync::Arc;

use chrono::{DateTime, TimeZone as _, Utc};

use kgd_domain::{DiaryEntry, compile_url_rules};

use crate::SyncDiaryMessageUseCase;
use crate::ports::{
    MockAttachmentDownloader, MockClock, MockDiaryRepository, MockImageConverter, MockNotionApi,
};

/// テスト用に UTC の DateTime を作る。
pub(crate) fn utc(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
        .unwrap()
}

/// テスト用の日報エントリを作る。
pub(crate) fn entry(thread_id: u64, date: DateTime<Utc>) -> DiaryEntry {
    DiaryEntry {
        thread_id,
        page_id: format!("page-{}", thread_id),
        page_url: format!("https://notion.example/{}", thread_id),
        date,
        created_at: date,
    }
}

/// 固定時刻を返す MockClock を作る。
pub(crate) fn fixed_clock(now: DateTime<Utc>) -> MockClock {
    let mut clock = MockClock::new();
    clock.expect_now().returning(move || now);
    clock
}

/// IO を一切呼ばない前提の SyncDiaryMessageUseCase を作る。
pub(crate) fn empty_sync_service() -> Arc<SyncDiaryMessageUseCase> {
    text_sync_service(0)
}

/// テキスト同期が expected_syncs 回成功する SyncDiaryMessageUseCase を作る。
pub(crate) fn text_sync_service(expected_syncs: usize) -> Arc<SyncDiaryMessageUseCase> {
    let url_rules =
        compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
    let mut notion = MockNotionApi::new();
    notion
        .expect_append_blocks()
        .times(expected_syncs)
        .returning(|_, _| Ok(vec!["b1".to_string()]));
    let mut repo = MockDiaryRepository::new();
    repo.expect_insert_message_block()
        .times(expected_syncs)
        .returning(|_| Ok(()));
    Arc::new(SyncDiaryMessageUseCase::new(
        Arc::new(notion),
        Arc::new(repo),
        Arc::new(MockAttachmentDownloader::new()),
        Arc::new(MockImageConverter::new()),
        None,
        url_rules,
    ))
}
