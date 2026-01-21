use std::net::IpAddr;
use std::time::Duration;
use surge_ping::{Client, Config, PingIdentifier, PingSequence};

pub async fn ping(addr: IpAddr, timeout: Duration) -> bool {
    let client = match Client::new(&Config::default()) {
        Ok(client) => client,
        Err(_) => return false,
    };

    let mut pinger = client.pinger(addr, PingIdentifier(rand_id())).await;
    pinger.timeout(timeout);

    pinger.ping(PingSequence(0), &[]).await.is_ok()
}

fn rand_id() -> u16 {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    (duration.as_nanos() & 0xFFFF) as u16
}
