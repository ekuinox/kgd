//! kgd のプレゼンテーション層。Discord のイベントを受けてユースケースを呼び出し、
//! 結果をユーザー向けの表現に変換する。

mod discord;
mod owntracks;
mod presenter;

pub use discord::{
    DiscordController, DiscordControllerSettings, StatusNotifier, run_status_receiver,
};
pub use owntracks::{OwnTracksControllerSettings, owntracks_router};
pub use presenter::VersionInfo;
