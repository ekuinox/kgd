//! kgd のドメイン層。IO フレームワークに依存しない型と純粋ロジックを提供する。
//!
//! このクレートは serenity / sqlx / reqwest などの IO ライブラリに依存してはならない。

mod attachment;
mod blocks;
mod diary;
mod diary_post;
mod location;
mod maintenance;
mod message;
mod ogp;
mod owntracks;
mod relay;
mod server;
mod url_rules;

pub use attachment::{
    FileType, classify_file, guess_content_type, is_spoiler_attachment, replace_extension,
    spoiler_summary,
};
pub use blocks::{file_block_json, image_block_json, paragraph_block_json, toggle_block_json};
pub use diary::{
    DIARY_CLOSE_AND_NEW_BUTTON_ID, DiaryCalendar, DiaryEntry, MessageBlock, RelayedMessage,
};
pub use diary_post::{DiaryPost, DiaryPostImage, DiaryPostRecord};
pub use location::{
    Activity, LocationReportText, LocationSummary, OSM_ATTRIBUTION, TILE_SIZE, TilePlacement,
    TrackPoint, TrackSegment, Viewport, calendar_day_range, count_points, filter_accurate,
    fit_viewport, format_empty_location_report, format_location_report, group_by_calendar_day,
    haversine_m, simplify_segments, split_segments, sum_summaries, summarize, world_pixel,
};
pub use maintenance::{
    DiaryHourlySyncSlot, HourlySyncDecision, decide_hourly_sync, should_attempt_auto_close,
};
pub use message::{SyncAttachment, SyncMessage, ThreadState, merge_forwarded_content};
pub use ogp::{OgpMetadata, parse_ogp_metadata};
pub use owntracks::{OwnTracksMessage, parse_owntracks_message, sanitize_identifier};
pub use relay::{
    DISCORD_MESSAGE_CONTENT_LIMIT, assemble_relay_content, build_relay_content, message_link,
};
pub use server::{ServerStatus, ServerTarget};
pub use url_rules::{
    CompiledUrlRules, PatternConfig, UrlRuleConfig, apply_ogp_to_bookmark,
    build_rich_text_and_url_blocks, compile_url_rules,
};
