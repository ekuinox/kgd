//! 地図画像の描画を抽象化するポート。

use anyhow::Result;

use kgd_domain::{TrackSegment, Viewport};

/// 地図画像の描画を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait MapRenderer: Send + Sync {
    /// 表示範囲の地図に軌跡の区間を重ねた画像を PNG で返す。
    async fn render(&self, viewport: &Viewport, segments: &[TrackSegment]) -> Result<Vec<u8>>;
}
