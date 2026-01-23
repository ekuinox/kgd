use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

use macaddr::MacAddr6;
use thiserror::Error;
use wake_on_lan::MagicPacket;

/// Wake-on-LAN 操作で発生しうるエラー。
#[derive(Error, Debug)]
pub enum WolError {
    /// ネットワーク操作に失敗した場合のエラー
    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),
}

/// Wake-on-LAN 操作の結果型。
pub type Result<T> = std::result::Result<T, WolError>;

/// Send a Wake-on-LAN magic packet to the specified MAC address
///
/// # Arguments
/// * `mac_address` - MAC address
/// * `broadcast_addr` - Optional broadcast address (default: "255.255.255.255:9")
pub fn send_wol_packet(mac_address: MacAddr6, broadcast_addr: Option<SocketAddr>) -> Result<()> {
    let addr =
        broadcast_addr.unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, 9)));
    let magic_packet = MagicPacket::new(&mac_address.into_array());
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    socket.send_to(magic_packet.magic_bytes(), addr)?;
    Ok(())
}
