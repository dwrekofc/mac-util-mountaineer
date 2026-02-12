use anyhow::{bail, Result};
use std::net::UdpSocket;

/// Parse a MAC address string (colon or hyphen separated) into 6 bytes.
fn parse_mac(mac: &str) -> Result<[u8; 6]> {
    let parts: Vec<&str> = mac.split(|c| c == ':' || c == '-').collect();
    if parts.len() != 6 {
        bail!("Invalid MAC address: expected 6 octets, got {}", parts.len());
    }

    let mut bytes = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        bytes[i] = u8::from_str_radix(part, 16)
            .map_err(|_| anyhow::anyhow!("Invalid hex octet '{}' in MAC address", part))?;
    }
    Ok(bytes)
}

/// Build a Wake-on-LAN magic packet: 6 bytes of 0xFF followed by the MAC repeated 16 times.
fn build_magic_packet(mac: &[u8; 6]) -> [u8; 102] {
    let mut packet = [0xFFu8; 102];
    for i in 0..16 {
        let offset = 6 + i * 6;
        packet[offset..offset + 6].copy_from_slice(mac);
    }
    packet
}

/// Send a Wake-on-LAN magic packet to the broadcast address.
pub fn send_wol(mac_address: &str) -> Result<()> {
    let mac = parse_mac(mac_address)?;
    let packet = build_magic_packet(&mac);

    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_broadcast(true)?;
    socket.send_to(&packet, "255.255.255.255:9")?;

    log::info!("Sent WoL magic packet to {}", mac_address);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_colon_separated() {
        let mac = parse_mac("d0:11:e5:13:af:1f").unwrap();
        assert_eq!(mac, [0xd0, 0x11, 0xe5, 0x13, 0xaf, 0x1f]);
    }

    #[test]
    fn parse_mac_hyphen_separated() {
        let mac = parse_mac("d0-11-e5-13-af-1f").unwrap();
        assert_eq!(mac, [0xd0, 0x11, 0xe5, 0x13, 0xaf, 0x1f]);
    }

    #[test]
    fn parse_mac_invalid() {
        assert!(parse_mac("invalid").is_err());
        assert!(parse_mac("d0:11:e5:13:af").is_err()); // too few
        assert!(parse_mac("d0:11:e5:13:af:1f:00").is_err()); // too many
        assert!(parse_mac("zz:11:e5:13:af:1f").is_err()); // bad hex
    }

    #[test]
    fn magic_packet_structure() {
        let mac = [0xd0, 0x11, 0xe5, 0x13, 0xaf, 0x1f];
        let packet = build_magic_packet(&mac);

        // First 6 bytes are 0xFF
        assert_eq!(&packet[0..6], &[0xFF; 6]);

        // MAC repeated 16 times
        for i in 0..16 {
            let offset = 6 + i * 6;
            assert_eq!(&packet[offset..offset + 6], &mac);
        }

        assert_eq!(packet.len(), 102);
    }
}
