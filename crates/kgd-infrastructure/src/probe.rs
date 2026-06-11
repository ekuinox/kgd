//! ICMP ping による ServerProber ポートの実装。

use std::{net::IpAddr, time::Duration};

use surge_ping::{Client, Config, PingIdentifier, PingSequence};

use kgd_application::ports::ServerProber;

/// surge-ping を使った [`ServerProber`] 実装。
#[derive(Debug, Clone, Copy, Default)]
pub struct SurgeProber;

#[async_trait::async_trait]
impl ServerProber for SurgeProber {
    async fn probe(&self, addr: IpAddr, timeout: Duration) -> bool {
        ping(addr, timeout).await
    }
}

/// 指定されたIPアドレスにICMP pingを送信し、到達可能かどうかを判定する。
///
/// # Arguments
/// * `addr` - pingを送信する対象のIPアドレス
/// * `timeout` - 応答を待機する最大時間
///
/// # Returns
/// サーバーが応答した場合は `true`、タイムアウトまたはエラーの場合は `false`
async fn ping(addr: IpAddr, timeout: Duration) -> bool {
    let client = match Client::new(&Config::default()) {
        Ok(client) => client,
        Err(_) => return false,
    };

    let mut pinger = client.pinger(addr, PingIdentifier(rand_id())).await;
    pinger.timeout(timeout);

    pinger.ping(PingSequence(0), &[]).await.is_ok()
}

/// ping識別子として使用するランダムなIDを生成する。
///
/// 現在時刻のナノ秒を元に16ビットの識別子を生成する。
fn rand_id() -> u16 {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (duration.as_nanos() & 0xFFFF) as u16
}
