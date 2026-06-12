//! UDP ブロードキャストによる WolSender ポートの実装。

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};

use anyhow::{Context as _, Result};
use macaddr::MacAddr6;
use wake_on_lan::MagicPacket;

use kgd_application::ports::WolSender;

/// UdpSocket で WOL マジックパケットを送信する [`WolSender`] 実装。
#[derive(Debug, Clone, Copy, Default)]
pub struct UdpWolSender;

impl WolSender for UdpWolSender {
    fn send_wol(&self, mac_address: MacAddr6) -> Result<()> {
        send_wol_packet(mac_address, None).context("Failed to send WOL packet")
    }
}

/// Wake-on-LAN マジックパケットを指定の MAC アドレス宛に送信する。
///
/// # Arguments
/// * `mac_address` - MAC アドレス
/// * `broadcast_addr` - 送信先ブロードキャストアドレス（デフォルト: "255.255.255.255:9"）
fn send_wol_packet(
    mac_address: MacAddr6,
    broadcast_addr: Option<SocketAddr>,
) -> std::io::Result<()> {
    let addr =
        broadcast_addr.unwrap_or_else(|| SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, 9)));
    let magic_packet = MagicPacket::new(&mac_address.into_array());
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    socket.send_to(magic_packet.magic_bytes(), addr)?;
    Ok(())
}
