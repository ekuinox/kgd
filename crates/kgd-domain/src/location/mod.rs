//! 位置ログ (OwnTracks) の集計と描画準備のための純粋ロジック。

mod track;

pub use track::{Activity, TrackPoint, TrackSegment, filter_accurate, split_segments};
