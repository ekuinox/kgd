use macaddr::MacAddr6;
use std::net::{Ipv4Addr, SocketAddr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WolError {
    #[error("Invalid MAC address format: {0}")]
    InvalidMacAddress(String),

    #[error("Network error: {0}")]
    NetworkError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, WolError>;

/// Send a Wake-on-LAN magic packet to the specified MAC address
///
/// # Arguments
/// * `mac_address` - MAC address in format "AA:BB:CC:DD:EE:FF"
/// * `broadcast_addr` - Optional broadcast address (default: "255.255.255.255:9")
pub fn send_wol_packet(mac_address: &str, broadcast_addr: Option<&str>) -> Result<()> {
    // Parse MAC address
    let mac = parse_mac_address(mac_address)?;

    // Parse broadcast address or use default
    let addr: SocketAddr = if let Some(addr_str) = broadcast_addr {
        addr_str.parse().map_err(|_| {
            WolError::NetworkError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid broadcast address",
            ))
        })?
    } else {
        SocketAddr::from((Ipv4Addr::BROADCAST, 9))
    };

    // Send WOL packet
    wol::send_magic_packet(mac, None, addr)?;

    Ok(())
}

/// Parse MAC address from string format
fn parse_mac_address(mac_str: &str) -> Result<MacAddr6> {
    let parts: Vec<&str> = mac_str.split(':').collect();

    if parts.len() != 6 {
        return Err(WolError::InvalidMacAddress(format!(
            "Expected 6 octets, got {}",
            parts.len()
        )));
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| WolError::InvalidMacAddress(format!("Invalid hex value: {}", part)))?;
    }

    Ok(MacAddr6::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mac_address() {
        let mac = parse_mac_address("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(mac, MacAddr6::from([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]));
    }

    #[test]
    fn test_invalid_mac_address() {
        assert!(parse_mac_address("AA:BB:CC:DD:EE").is_err());
        assert!(parse_mac_address("AA:BB:CC:DD:EE:GG").is_err());
    }
}
