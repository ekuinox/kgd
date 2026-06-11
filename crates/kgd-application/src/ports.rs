//! アプリケーション層が依存する外部 IO の抽象 (ポート)。
//!
//! 具体実装 (アダプタ) は infrastructure 側で提供する。
//! すべての trait は `#[cfg_attr(test, mockall::automock)]` を付与しており、
//! テストでは `MockNotionApi` などの自動生成モックを使用できる。

use std::{collections::HashMap, net::IpAddr, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Utc};
use macaddr::MacAddr6;

use kgd_domain::{
    DiaryEntry, MessageBlock, OgpMetadata, RelayedMessage, SyncAttachment, SyncMessage, ThreadState,
};

/// Notion API へのアクセスを抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait NotionApi: Send + Sync {
    /// 指定したタイトルの日報ページを検索し、存在すれば (ページ ID, URL) を返す。
    async fn find_diary_page_by_title(&self, title: &str) -> Result<Option<(String, String)>>;

    /// 日報ページを作成し、(ページ ID, URL) を返す。
    async fn create_diary_page(&self, title: &str) -> Result<(String, String)>;

    /// ファイルをアップロードし、ファイルアップロード ID を返す。
    async fn upload_file(
        &self,
        filename: &str,
        content_type: &str,
        data: Vec<u8>,
    ) -> Result<String>;

    /// 複数のブロックを一括でページに追加し、作成されたブロック ID のリストを返す。
    async fn append_blocks(
        &self,
        page_id: &str,
        children: Vec<serde_json::Value>,
    ) -> Result<Vec<String>>;

    /// テキストブロックの rich_text を更新する。
    async fn update_text_block(
        &self,
        block_id: &str,
        rich_text: Vec<serde_json::Value>,
    ) -> Result<()>;

    /// ブロックを削除する。
    async fn delete_block(&self, block_id: &str) -> Result<()>;
}

/// 日報エントリとメッセージブロックの永続化を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait DiaryRepository: Send + Sync {
    /// エントリを追加する (thread_id が重複する場合は上書き)。
    async fn insert(&self, entry: &DiaryEntry) -> Result<()>;

    /// スレッド ID からエントリを取得する。
    async fn get_by_thread(&self, thread_id: u64) -> Result<Option<DiaryEntry>>;

    /// 日付からエントリを取得する。
    async fn get_by_date(&self, date: DateTime<Utc>) -> Result<Option<DiaryEntry>>;

    /// メッセージとブロックの対応を保存する。
    async fn insert_message_block(&self, block: &MessageBlock) -> Result<()>;

    /// メッセージ ID から対応するブロック一覧を取得する。
    async fn get_blocks_by_message(&self, message_id: u64) -> Result<Vec<MessageBlock>>;

    /// メッセージ ID に対応するブロックをすべて削除する。
    async fn delete_blocks_by_message(&self, message_id: u64) -> Result<()>;

    /// メッセージ ID に紐づくブロックが存在するかどうかを返す。
    async fn has_blocks_by_message(&self, message_id: u64) -> Result<bool>;

    /// 指定した日付範囲に含まれるエントリを古い順で取得する (両端を含む)。
    async fn get_entries_in_date_range(
        &self,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<Vec<DiaryEntry>>;

    /// 最新の日報エントリを取得する。
    async fn get_latest_entry(&self) -> Result<Option<DiaryEntry>>;

    /// 元メッセージと転記メッセージの対応を保存する。
    ///
    /// 同じ元メッセージに対しては転記先の情報を上書きする。
    async fn upsert_relayed_message(&self, relayed: &RelayedMessage) -> Result<()>;

    /// 元メッセージ ID から転記メッセージの対応を取得する。
    async fn get_relayed_message(&self, source_message_id: u64) -> Result<Option<RelayedMessage>>;

    /// 元メッセージ ID に対応する転記メッセージの対応を削除する。
    async fn delete_relayed_message(&self, source_message_id: u64) -> Result<()>;
}

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

/// OGP メタデータの取得を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait OgpClient: Send + Sync {
    /// 複数 URL の OGP メタデータを並列で取得する。
    ///
    /// 取得に失敗した URL は結果に含まれない。
    async fn fetch_many(&self, urls: &[String]) -> HashMap<String, OgpMetadata>;
}

/// 添付ファイルのダウンロードを抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait AttachmentDownloader: Send + Sync {
    /// 添付ファイルをダウンロードし、(バイト列, Content-Type) を返す。
    async fn download(&self, attachment: &SyncAttachment) -> Result<(Vec<u8>, String)>;
}

/// 画像形式の変換を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
pub trait ImageConverter: Send + Sync {
    /// HEIC/HEIF 画像を JPEG に変換する。
    ///
    /// 変換がサポートされない環境ではエラーを返す。
    fn heic_to_jpeg(&self, data: &[u8]) -> Result<Vec<u8>>;
}

/// 現在時刻の取得を抽象化するポート。
///
/// 時刻に依存する判定ロジックをテスト可能にするために使用する。
#[cfg_attr(test, mockall::automock)]
pub trait Clock: Send + Sync {
    /// 現在時刻 (UTC) を返す。
    fn now(&self) -> DateTime<Utc>;
}

/// Wake-on-LAN パケットの送信を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
pub trait WolSender: Send + Sync {
    /// 指定した MAC アドレス宛に WOL マジックパケットを送信する。
    fn send_wol(&self, mac_address: MacAddr6) -> Result<()>;
}

/// サーバーの死活確認を抽象化するポート。
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ServerProber: Send + Sync {
    /// 指定アドレスに ping を送り、応答があれば `true` を返す。
    async fn probe(&self, addr: IpAddr, timeout: Duration) -> bool;
}
