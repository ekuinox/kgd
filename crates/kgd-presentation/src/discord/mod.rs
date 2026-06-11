//! Discord のイベントを受けてユースケースを呼び出すコントローラ。

use std::sync::Arc;

use tokio::sync::mpsc;

use kgd_application::{
    ManageDiaryLifecycle, RunDiaryMaintenance, SyncDiaryMessage, WakeServer, WriteChannelEvent,
    ports::DiaryRepository,
};
use kgd_domain::ServerTarget;

use crate::presenter::VersionInfo;

mod commands;
mod components;
mod diary_commands;
mod events;
mod messages;
mod status;
mod write_channel;

#[cfg(test)]
mod tests;

pub use status::{StatusNotifier, run_status_receiver};

/// コマンド実行が許可されているか判定する純粋関数。
///
/// 管理者リストが空の場合は全員許可。空でない場合はリストに含まれるユーザーのみ許可。
fn is_authorized(admins: &[u64], user_id: u64) -> bool {
    admins.is_empty() || admins.contains(&user_id)
}

/// Handler の表示・認可まわりの設定。
#[derive(Debug, Clone)]
pub struct HandlerSettings {
    /// コマンド実行を許可する管理者のユーザー ID 一覧
    pub admins: Vec<u64>,
    /// バージョン情報
    pub version_info: VersionInfo,
    /// 操作対象のサーバー一覧
    pub servers: Vec<ServerTarget>,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
}

/// Discord イベントを処理するハンドラー。
#[derive(Clone)]
pub struct Handler {
    /// 表示・認可まわりの設定
    pub(crate) settings: HandlerSettings,
    /// 日報リポジトリ
    pub(crate) diary_store: Arc<dyn DiaryRepository>,
    /// メッセージ同期ユースケース
    pub(crate) sync_service: Arc<SyncDiaryMessage>,
    /// 定期メンテナンスユースケース
    pub(crate) maintenance: Arc<RunDiaryMaintenance>,
    /// 日報ライフサイクルユースケース
    pub(crate) lifecycle: Arc<ManageDiaryLifecycle>,
    /// 書き込み用チャンネルイベントの送信キュー
    pub(crate) relay_tx: mpsc::Sender<WriteChannelEvent>,
    /// WOL ユースケース
    pub(crate) wake_server: Arc<WakeServer>,
}

impl Handler {
    /// 新しい Handler を作成する。
    pub fn new(
        settings: HandlerSettings,
        diary_store: Arc<dyn DiaryRepository>,
        sync_service: Arc<SyncDiaryMessage>,
        maintenance: Arc<RunDiaryMaintenance>,
        lifecycle: Arc<ManageDiaryLifecycle>,
        relay_tx: mpsc::Sender<WriteChannelEvent>,
        wake_server: Arc<WakeServer>,
    ) -> Self {
        Self {
            settings,
            diary_store,
            sync_service,
            maintenance,
            lifecycle,
            relay_tx,
            wake_server,
        }
    }
}
