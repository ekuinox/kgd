//! 位置ログ (OwnTracks) の集計と描画準備のための純粋ロジック。

mod history;
mod projection;
mod report_text;
mod summary;
mod track;

pub use history::{calendar_day_range, group_by_calendar_day, sum_summaries};
pub use projection::{TILE_SIZE, TilePlacement, Viewport, fit_viewport, world_pixel};
pub use report_text::{
    LocationReportText, OSM_ATTRIBUTION, format_empty_location_report, format_location_report,
};
pub use summary::{LocationSummary, haversine_m, summarize};
pub use track::{Activity, TrackPoint, TrackSegment, filter_accurate, split_segments};
