//! アプリケーション層が依存する外部 IO の抽象 (ポート)。
//!
//! 具体実装 (アダプタ) は infrastructure 側で提供する。
//! すべての trait は `#[cfg_attr(test, mockall::automock)]` を付与しており、
//! テストでは `MockNotionApi` などの自動生成モックを使用できる。

mod discord;
mod misc;
mod notion;
mod repository;

pub use discord::DiscordGateway;
pub use misc::{AttachmentDownloader, Clock, ImageConverter, OgpClient, ServerProber, WolSender};
pub use notion::NotionApi;
pub use repository::DiaryRepository;

#[cfg(test)]
pub use discord::MockDiscordGateway;
#[cfg(test)]
pub use misc::{
    MockAttachmentDownloader, MockClock, MockImageConverter, MockOgpClient, MockServerProber,
    MockWolSender,
};
#[cfg(test)]
pub use notion::MockNotionApi;
#[cfg(test)]
pub use repository::MockDiaryRepository;
