use std::net::IpAddr;
use std::time::Duration;

use tracing::info;

use crate::config::ServerConfig;
use crate::ping::ping;

pub struct ServerStatus {
    pub name: String,
    pub online: bool,
}

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
