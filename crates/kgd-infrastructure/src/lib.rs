//! kgd のインフラ層。アプリケーション層のポートに対する具体実装 (アダプタ) を提供する。
//!
//! serenity / sqlx / reqwest などの IO ライブラリへの依存はこのクレートに閉じ込める。

mod clock;
mod diary_post_store;
mod discord_gateway;
mod downloader;
mod http_server;
mod image_converter;
mod location_store;
mod map_renderer;
mod notion;
mod ogp;
mod probe;
mod scheduler;
mod store;
mod wol_sender;

pub use clock::SystemClock;
pub use diary_post_store::DiaryPostStore;
pub use discord_gateway::{SerenityGateway, to_sync_message};
pub use downloader::ReqwestDownloader;
pub use http_server::{bind_http, serve_http};
pub use image_converter::HeifConverter;
pub use location_store::LocationStore;
pub use map_renderer::TileMapRenderer;
pub use notion::{NotionClient, NotionTagConfig};
pub use ogp::OgpFetcher;
pub use probe::SurgeProber;
pub use scheduler::Scheduler;
pub use store::{DiaryStore, connect_pool};
pub use wol_sender::UdpWolSender;
