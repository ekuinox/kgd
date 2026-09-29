//! Discord のイベントを受けてユースケースを呼び出すコントローラ。

use std::sync::Arc;

use tokio::sync::mpsc;

use kgd_application::{
    BuildLocationReportUseCase, ManageDiaryLifecycleUseCase, RunDiaryMaintenanceUseCase,
    SyncDiaryMessageUseCase, WakeServerUseCase, WriteChannelEvent,
    ports::{Clock, DiaryRepository},
};
use kgd_domain::{DiaryCalendar, ServerTarget};

use crate::presenter::VersionInfo;

mod commands;
mod components;
mod diary_commands;
mod events;
mod location_commands;
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

/// DiscordController の表示・認可まわりの設定。
#[derive(Debug, Clone)]
pub struct DiscordControllerSettings {
    /// コマンド実行を許可する管理者のユーザー ID 一覧
    pub admins: Vec<u64>,
    /// バージョン情報
    pub version_info: VersionInfo,
    /// 操作対象のサーバー一覧
    pub servers: Vec<ServerTarget>,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
}

/// `/location report` に必要な依存。
///
/// `[location]` が無いときは作らず、コマンドも登録しない。
#[derive(Clone)]
pub struct LocationReportCommand {
    /// レポートを作るユースケース
    pub build: Arc<BuildLocationReportUseCase>,
    /// 日報日の区切り方
    pub calendar: DiaryCalendar,
    /// 時刻ポート
    pub clock: Arc<dyn Clock>,
}

/// Discord イベントを処理するハンドラー。
#[derive(Clone)]
pub struct DiscordController {
    /// 表示・認可まわりの設定
    pub(crate) settings: DiscordControllerSettings,
    /// 日報リポジトリ
    pub(crate) diary_store: Arc<dyn DiaryRepository>,
    /// メッセージ同期ユースケース
    pub(crate) sync_service: Arc<SyncDiaryMessageUseCase>,
    /// 定期メンテナンスユースケース
    pub(crate) maintenance: Arc<RunDiaryMaintenanceUseCase>,
    /// 日報ライフサイクルユースケース
    pub(crate) lifecycle: Arc<ManageDiaryLifecycleUseCase>,
    /// 書き込み用チャンネルイベントの送信キュー
    pub(crate) relay_tx: mpsc::Sender<WriteChannelEvent>,
    /// WOL ユースケース
    pub(crate) wake_server: Arc<WakeServerUseCase>,
    /// 位置ログのレポートコマンド (未設定なら None)
    pub(crate) location_report: Option<LocationReportCommand>,
}

impl DiscordController {
    /// 新しい DiscordController を作成する。
    // 各ユースケースを個別の依存として受け取っており、この lint を避けるためだけに
    // まとめて構造体にすると、意味のある単位ではない引数の入れ物が増えるだけになる。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settings: DiscordControllerSettings,
        diary_store: Arc<dyn DiaryRepository>,
        sync_service: Arc<SyncDiaryMessageUseCase>,
        maintenance: Arc<RunDiaryMaintenanceUseCase>,
        lifecycle: Arc<ManageDiaryLifecycleUseCase>,
        relay_tx: mpsc::Sender<WriteChannelEvent>,
        wake_server: Arc<WakeServerUseCase>,
        location_report: Option<LocationReportCommand>,
    ) -> Self {
        Self {
            settings,
            diary_store,
            sync_service,
            maintenance,
            lifecycle,
            relay_tx,
            wake_server,
            location_report,
        }
    }
}
