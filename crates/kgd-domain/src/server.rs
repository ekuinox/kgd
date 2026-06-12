//! 監視対象サーバーのドメイン型。

use macaddr::MacAddr6;

/// 監視・操作対象のサーバー情報。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerTarget {
    /// サーバー名（識別用）
    pub name: String,
    /// Wake-on-LAN 送信先の MAC アドレス
    pub mac_address: MacAddr6,
    /// ping 送信先の IP アドレス
    pub ip_address: String,
    /// サーバーの説明文
    pub description: String,
}

/// サーバーの死活状態。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    /// サーバー名
    pub name: String,
    /// オンラインかどうか
    pub online: bool,
}
