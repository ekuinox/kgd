//! 位置ログ (OwnTracks) の集計と描画準備のための純粋ロジック。

mod summary;
mod track;

pub use summary::{LocationSummary, haversine_m, summarize};
pub use track::{Activity, TrackPoint, TrackSegment, filter_accurate, split_segments};
