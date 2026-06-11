//! 同期処理が扱うメッセージのドメイン型。
//!
//! serenity の型をアプリケーションロジックに持ち込まないための最小表現。
//! serenity からの変換は infrastructure 側が行う。

/// 同期対象メッセージの最小表現。
///
/// Discord の `Message` から必要な情報だけを写し取った DTO。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMessage {
    /// Discord メッセージ ID
    pub message_id: u64,
    /// メッセージが属するチャンネル (スレッド) ID
    pub channel_id: u64,
    /// メッセージ本文
    pub content: String,
    /// Bot が送信したメッセージかどうか
    pub is_bot: bool,
    /// 添付ファイル一覧
    pub attachments: Vec<SyncAttachment>,
}

/// 同期対象メッセージの添付ファイルの最小表現。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAttachment {
    /// ファイル名
    pub filename: String,
    /// ダウンロード URL
    pub url: String,
    /// 添付ファイルの説明 (ALT テキスト)
    pub description: Option<String>,
}

/// Discord スレッドの状態。
///
/// 自動クローズ判定などで使用する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadState {
    /// 公開スレッドかどうか
    pub is_public_thread: bool,
    /// アーカイブ済みかどうか
    pub archived: bool,
    /// ロック済みかどうか
    pub locked: bool,
}

impl ThreadState {
    /// アーカイブまたはロックされている (クローズ扱いの) 状態かどうかを返す。
    pub fn is_closed(&self) -> bool {
        self.archived || self.locked
    }
}
