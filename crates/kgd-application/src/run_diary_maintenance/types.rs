//! 定期メンテナンスの設定と結果レポートの型。

use kgd_domain::DiaryCalendar;

/// 定期メンテナンスの設定。
#[derive(Debug, Clone)]
pub struct DiaryMaintenanceSettings {
    /// 日報の日付計算に使用するカレンダー
    pub calendar: DiaryCalendar,
    /// 自動クローズ機能を有効にするか
    pub auto_close_enabled: bool,
    /// 日報の書き込み用チャンネル ID
    pub write_channel_id: u64,
    /// 同期成功時にメッセージに付けるリアクション絵文字
    pub sync_reaction: String,
}

/// 日報スレッド走査の結果レポート。
#[derive(Debug, Clone, Copy, Default)]
pub struct DiaryThreadSyncReport {
    /// 確認したメッセージ数
    pub checked_messages: usize,
    /// 新規に同期したメッセージ数
    pub synced_messages: usize,
    /// 既に同期済みだったメッセージ数
    pub already_synced_messages: usize,
    /// スキップしたメッセージ数
    pub skipped_messages: usize,
}
