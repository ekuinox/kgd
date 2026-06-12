//! Wake-on-LAN でサーバーを起動するユースケース。

use std::sync::Arc;

use anyhow::Result;
use macaddr::MacAddr6;
use tracing::info;

use kgd_domain::ServerTarget;

use super::ports::WolSender;

/// WOL 送信の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeOutcome {
    /// 指定された名前のサーバーが見つからなかった
    ServerNotFound,
    /// WOL パケットを送信した
    Sent {
        /// サーバー名
        name: String,
        /// 送信先 MAC アドレス
        mac_address: MacAddr6,
    },
}

/// Wake-on-LAN でサーバーを起動するユースケース。
pub struct WakeServerUseCase {
    /// WOL 送信ポート
    wol: Arc<dyn WolSender>,
    /// 操作対象のサーバー一覧
    servers: Vec<ServerTarget>,
}

impl WakeServerUseCase {
    /// 新しい WakeServerUseCase を作成する。
    pub fn new(wol: Arc<dyn WolSender>, servers: Vec<ServerTarget>) -> Self {
        Self { wol, servers }
    }

    /// 指定した名前のサーバーへ WOL パケットを送信する。
    pub fn wake(&self, server_name: &str) -> Result<WakeOutcome> {
        let Some(server) = self.servers.iter().find(|s| s.name == server_name) else {
            return Ok(WakeOutcome::ServerNotFound);
        };

        self.wol.send_wol(server.mac_address)?;
        info!(server = %server.name, mac = %server.mac_address, "WOL packet sent");

        Ok(WakeOutcome::Sent {
            name: server.name.clone(),
            mac_address: server.mac_address,
        })
    }
}

#[cfg(test)]
mod tests {
    use mockall::predicate::eq;

    use crate::ports::MockWolSender;

    use super::*;

    fn server(name: &str, mac: MacAddr6) -> ServerTarget {
        ServerTarget {
            name: name.to_string(),
            mac_address: mac,
            ip_address: "192.168.1.10".to_string(),
            description: String::new(),
        }
    }

    #[test]
    fn wake_sends_wol_to_known_server() {
        let mac = MacAddr6::new(0, 1, 2, 3, 4, 5);
        let mut wol = MockWolSender::new();
        wol.expect_send_wol()
            .with(eq(mac))
            .times(1)
            .returning(|_| Ok(()));

        let use_case = WakeServerUseCase::new(Arc::new(wol), vec![server("nas", mac)]);
        let outcome = use_case.wake("nas").unwrap();

        assert_eq!(
            outcome,
            WakeOutcome::Sent {
                name: "nas".to_string(),
                mac_address: mac,
            }
        );
    }

    #[test]
    fn wake_returns_not_found_for_unknown_server() {
        let mut wol = MockWolSender::new();
        wol.expect_send_wol().times(0);

        let use_case = WakeServerUseCase::new(
            Arc::new(wol),
            vec![server("nas", MacAddr6::new(0, 1, 2, 3, 4, 5))],
        );
        let outcome = use_case.wake("unknown").unwrap();

        assert_eq!(outcome, WakeOutcome::ServerNotFound);
    }
}
