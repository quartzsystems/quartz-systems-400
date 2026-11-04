// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's socket: UDP port 68 bound to one link
//! (`SO_BINDTODEVICE`), broadcast allowed. The kernel takes the
//! servers' broadcast answers on a link with no address yet because the
//! client sets the broadcast flag; renewals go unicast from the leased
//! address. Off Linux opening one fails.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Socket;

#[cfg(not(target_os = "linux"))]
mod host;
#[cfg(not(target_os = "linux"))]
pub use host::Socket;

use std::io;

/// A link's Ethernet address from `/sys/class/net/<link>/address`.
pub fn mac_of(link: &str) -> io::Result<[u8; 6]> {
    let text = std::fs::read_to_string(format!("/sys/class/net/{link}/address"))?;
    parse_mac(&text).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("{link}: no Ethernet address")))
}

/// `52:54:00:12:34:56`.
pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let parts: Vec<u8> = text
        .trim()
        .split(':')
        .map(|p| u8::from_str_radix(p, 16).ok())
        .collect::<Option<_>>()?;
    parts.try_into().ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn macs() {
        assert_eq!(super::parse_mac("52:54:00:12:34:56\n"), Some([0x52, 0x54, 0, 0x12, 0x34, 0x56]));
        assert_eq!(super::parse_mac("00:00"), None);
        assert_eq!(super::parse_mac("zz:54:00:12:34:56"), None);
    }
}
