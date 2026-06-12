//! OGP 取得・添付ダウンロード・画像変換・時刻・WOL・死活確認のポート。

use std::{collections::HashMap, net::IpAddr, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Utc};
use macaddr::MacAddr6;

use kgd_domain::{OgpMetadata, SyncAttachment};

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
