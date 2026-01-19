use macaddr::MacAddr6;
use std::net::{Ipv4Addr, SocketAddr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WolError {
    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WolError>;

/// Send a Wake-on-LAN magic packet to the specified MAC address
///
/// # Arguments
/// * `mac_address` - MAC address
/// * `broadcast_addr` - Optional broadcast address (default: "255.255.255.255:9")
pub fn send_wol_packet(mac_address: MacAddr6, broadcast_addr: Option<SocketAddr>) -> Result<()> {
    let addr = broadcast_addr.unwrap_or_else(|| SocketAddr::from((Ipv4Addr::BROADCAST, 9)));
    wol::send_magic_packet(mac_address, None, addr)?;
    Ok(())
}
