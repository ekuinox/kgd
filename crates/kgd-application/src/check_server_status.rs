//! サーバーの死活確認を行うユースケース。

use std::{sync::Arc, time::Duration};

use kgd_domain::{ServerStatus, ServerTarget};

use super::ports::ServerProber;

/// サーバーの死活確認を行うユースケース。
pub struct CheckServerStatus {
    /// 死活確認ポート
    prober: Arc<dyn ServerProber>,
    /// 監視対象のサーバー一覧
    servers: Vec<ServerTarget>,
    /// ping のタイムアウト
    timeout: Duration,
}

impl CheckServerStatus {
    /// 新しい CheckServerStatus を作成する。
    pub fn new(
        prober: Arc<dyn ServerProber>,
        servers: Vec<ServerTarget>,
        timeout: Duration,
    ) -> Self {
        Self {
            prober,
            servers,
            timeout,
        }
    }

    /// 全サーバーの死活状態を確認する。
    ///
    /// IP アドレスがパースできないサーバーはオフライン扱いとする。
    pub async fn check_all(&self) -> Vec<ServerStatus> {
        let mut results = Vec::with_capacity(self.servers.len());

        for server in &self.servers {
            let online = match server.ip_address.parse() {
                Ok(ip) => self.prober.probe(ip, self.timeout).await,
                Err(_) => false,
            };
            results.push(ServerStatus {
                name: server.name.clone(),
                online,
            });
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use macaddr::MacAddr6;

    use crate::ports::MockServerProber;

    use super::*;

    fn server(name: &str, ip: &str) -> ServerTarget {
        ServerTarget {
            name: name.to_string(),
            mac_address: MacAddr6::new(0, 1, 2, 3, 4, 5),
            ip_address: ip.to_string(),
            description: String::new(),
        }
    }

    #[tokio::test]
    async fn check_all_reports_each_server() {
        let mut prober = MockServerProber::new();
        prober
            .expect_probe()
            .withf(|ip, _| ip.to_string() == "192.168.1.10")
            .times(1)
            .returning(|_, _| true);
        prober
            .expect_probe()
            .withf(|ip, _| ip.to_string() == "192.168.1.11")
            .times(1)
            .returning(|_, _| false);

        let use_case = CheckServerStatus::new(
            Arc::new(prober),
            vec![
                server("a", "192.168.1.10"),
                server("b", "192.168.1.11"),
                // パース不能な IP はオフライン扱い（probe は呼ばれない）
                server("c", "invalid-ip"),
            ],
            Duration::from_secs(5),
        );

        let statuses = use_case.check_all().await;

        assert_eq!(
            statuses,
            vec![
                ServerStatus {
                    name: "a".to_string(),
                    online: true
                },
                ServerStatus {
                    name: "b".to_string(),
                    online: false
                },
                ServerStatus {
                    name: "c".to_string(),
                    online: false
                },
            ]
        );
    }
}
