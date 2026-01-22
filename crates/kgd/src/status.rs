//! サーバーステータスの監視機能を提供する。
//!
//! 設定されたサーバー一覧に対してpingを実行し、オンライン/オフライン状態を取得する。

use std::{net::IpAddr, time::Duration};

use tracing::info;

use crate::{config::ServerConfig, ping::ping};

/// サーバーのステータス情報を表す構造体。
pub struct ServerStatus {
    /// サーバー名
    pub name: String,
    /// オンライン状態 (`true`: オンライン, `false`: オフライン)
    pub online: bool,
}

/// 複数のサーバーに対してpingを実行し、それぞれのステータスを取得する。
///
/// # Arguments
/// * `servers` - チェック対象のサーバー設定リスト
/// * `timeout` - 各サーバーへのping待機時間
///
/// # Returns
/// 各サーバーのステータス情報のリスト
pub async fn check_servers(servers: &[ServerConfig], timeout: Duration) -> Vec<ServerStatus> {
    info!("Checking server status");

    let mut results = Vec::with_capacity(servers.len());

    for server in servers {
        let online = match server.ip_address.parse::<IpAddr>() {
            Ok(ip) => ping(ip, timeout).await,
            Err(_) => false,
        };

        info!(server = %server.name, online, "Server status checked");
        results.push(ServerStatus {
            name: server.name.clone(),
            online,
        });
    }

    results
}
