//! kgd のドメイン層。IO フレームワークに依存しない型と純粋ロジックを提供する。
//!
//! このクレートは serenity / sqlx / reqwest などの IO ライブラリに依存してはならない。

mod attachment;
mod blocks;
mod diary;
mod maintenance;
mod message;
mod ogp;
mod server;
mod url_rules;

pub use attachment::{
    FileType, classify_file, guess_content_type, is_spoiler_attachment, replace_extension,
    spoiler_summary,
};
pub use blocks::{file_block_json, image_block_json, toggle_block_json};
pub use diary::{
    DIARY_CLOSE_AND_NEW_BUTTON_ID, DiaryEntry, MessageBlock, format_date_in_timezone,
    today_in_timezone,
};
pub use maintenance::{
    DiaryHourlySyncSlot, HourlySyncDecision, decide_hourly_sync, should_attempt_auto_close,
    should_send_auto_close,
};
pub use message::{SyncAttachment, SyncMessage, ThreadState, merge_forwarded_content};
pub use ogp::{OgpMetadata, parse_ogp_metadata};
pub use server::{ServerStatus, ServerTarget};
pub use url_rules::{
    CompiledUrlRules, PatternConfig, UrlRuleConfig, apply_ogp_to_bookmark,
    build_rich_text_and_url_blocks, compile_url_rules,
};
