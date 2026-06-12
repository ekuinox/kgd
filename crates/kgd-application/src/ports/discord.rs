//! Discord への操作を抽象化するポート。

use anyhow::Result;

use kgd_domain::{SyncMessage, ThreadState};

/// Discord への操作を抽象化するポート。
///
/// serenity の型はこの trait の境界で plain な型 (u64, String, ドメイン DTO) に変換する。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait DiscordGateway: Send + Sync {
    /// スレッドの状態を取得する。スレッドが取得できない場合は `None` を返す。
    async fn thread_state(&self, thread_id: u64) -> Result<Option<ThreadState>>;

    /// フォーラムにスレッドを作成し、作成された thread_id を返す。
    async fn create_diary_forum_post(
        &self,
        forum_channel_id: u64,
        title: &str,
        page_url: &str,
    ) -> Result<u64>;

    /// スレッドをアーカイブ & ロックする (クローズ)。
    async fn close_thread(&self, thread_id: u64) -> Result<()>;

    /// アーカイブ済みスレッドへ join して再開する。成功したかどうかを返す。
    async fn reopen_thread(&self, thread_id: u64) -> Result<bool>;

    /// 直近 limit 件のメッセージにクローズ & 新規作成ボタンが含まれるかを返す。
    async fn has_close_and_new_button(&self, thread_id: u64, limit: u8) -> Result<bool>;

    /// 平文メッセージを送信し、送信したメッセージ ID を返す。
    async fn send_text(&self, channel_id: u64, content: &str) -> Result<u64>;

    /// メッセージの本文を編集する。
    async fn edit_message_content(
        &self,
        channel_id: u64,
        message_id: u64,
        content: &str,
    ) -> Result<()>;

    /// メッセージを削除する。
    async fn delete_message(&self, channel_id: u64, message_id: u64) -> Result<()>;

    /// クローズ & 新規作成ボタン付きメッセージを送信する。
    async fn send_close_and_new_button(&self, thread_id: u64) -> Result<()>;

    /// 書き込み用チャンネルへ新しい日報作成のボタン付きメッセージを送信する。
    async fn send_write_channel_new_diary_button(&self, channel_id: u64) -> Result<()>;

    /// before より前のメッセージを limit 件取得する (新しい順)。
    ///
    /// `before` が `None` の場合は最新から取得する。
    async fn fetch_messages_before(
        &self,
        thread_id: u64,
        before: Option<u64>,
        limit: u8,
    ) -> Result<Vec<SyncMessage>>;

    /// メッセージにリアクションを付与する。
    async fn add_reaction(&self, channel_id: u64, message_id: u64, emoji: &str) -> Result<()>;
}
