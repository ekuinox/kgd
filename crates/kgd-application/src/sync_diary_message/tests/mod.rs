//! SyncDiaryMessageUseCase の単体テスト（共有ヘルパ）。

use kgd_domain::compile_url_rules;

use crate::ports::{
    MockAttachmentDownloader, MockDiaryRepository, MockImageConverter, MockNotionApi, MockOgpClient,
};

use super::*;

mod attachments;
mod edits;
mod heic;
mod sync;

/// テスト用のユースケースを構築するビルダー。
struct TestSyncBuilder {
    notion: MockNotionApi,
    repo: MockDiaryRepository,
    downloader: MockAttachmentDownloader,
    converter: MockImageConverter,
    ogp: Option<MockOgpClient>,
}

impl TestSyncBuilder {
    fn new() -> Self {
        Self {
            notion: MockNotionApi::new(),
            repo: MockDiaryRepository::new(),
            downloader: MockAttachmentDownloader::new(),
            converter: MockImageConverter::new(),
            ogp: None,
        }
    }

    fn build(self) -> SyncDiaryMessageUseCase {
        let url_rules =
            compile_url_rules(&[], &["link".to_string()]).expect("default rules should be ok");
        SyncDiaryMessageUseCase::new(
            Arc::new(self.notion),
            Arc::new(self.repo),
            Arc::new(self.downloader),
            Arc::new(self.converter),
            self.ogp.map(|ogp| Arc::new(ogp) as Arc<dyn OgpClient>),
            url_rules,
        )
    }
}

/// テスト用のテキストメッセージを作る。
fn text_message(message_id: u64, content: &str) -> SyncMessage {
    SyncMessage {
        message_id,
        channel_id: 1,
        guild_id: Some(1),
        content: content.to_string(),
        is_bot: false,
        attachments: vec![],
    }
}
