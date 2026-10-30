// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Addresses as the commands take them: dotted IPv4 (`'10.0.2.15'`),
//! IPv6 text, the special values, subnet masks and prefix lengths.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::{Error, Result};
use crate::msg;

/// An interface's `INTNETADR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IfcAddr {
    /// A static address.
    Ip(IpAddr),
    /// `*IP4DHCP`: an IPv4 address from DHCP.
    Dhcp4,
    /// `*IP6SAC`: IPv6 stateless address autoconfiguration.
    Slaac6,
}

impl IfcAddr {
    /// From the command's text.
    pub fn parse(s: &str) -> Result<Self> {
        let t = unquote(s);
        match t.to_ascii_uppercase().as_str() {
            "*IP4DHCP" => Ok(Self::Dhcp4),
            "*IP6SAC" => Ok(Self::Slaac6),
            _ => parse_ip(t).map(Self::Ip),
        }
    }

    /// The address, when static.
    pub fn ip(self) -> Option<IpAddr> {
        match self {
            Self::Ip(ip) => Some(ip),
            _ => None,
        }
    }

    /// An IPv6 interface (static or `*IP6SAC`).
    pub fn is_v6(self) -> bool {
        match self {
            Self::Ip(ip) => ip.is_ipv6(),
            Self::Slaac6 => true,
            Self::Dhcp4 => false,
        }
    }
}

impl fmt::Display for IfcAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ip(ip) => write!(f, "{ip}"),
            Self::Dhcp4 => f.write_str("*IP4DHCP"),
            Self::Slaac6 => f.write_str("*IP6SAC"),
        }
    }
}

/// A value without its quotes and blanks.
pub fn unquote(s: &str) -> &str {
    s.trim().trim_matches('\'').trim()
}

/// An IPv4 or IPv6 address; `TCP1D08` when it is neither.
pub fn parse_ip(s: &str) -> Result<IpAddr> {
    let t = unquote(s);
    if let Ok(v4) = parse_v4(t) {
        return Ok(IpAddr::V4(v4));
    }
    t.parse::<Ipv6Addr>()
        .map(IpAddr::V6)
        .map_err(|_| bad("INTNETADR", t))
}

/// A dotted IPv4 address: four decimal parts, leading zeros allowed as
/// IBM allows them (`010.000.002.015`).
pub fn parse_v4(s: &str) -> Result<Ipv4Addr> {
    let t = unquote(s);
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() != 4 {
        return Err(bad("INTNETADR", t));
    }
    let mut b = [0u8; 4];
    for (slot, p) in b.iter_mut().zip(&parts) {
        if p.is_empty() || p.len() > 3 || !p.bytes().all(|c| c.is_ascii_digit()) {
            return Err(bad("INTNETADR", t));
        }
        *slot = p.parse().map_err(|_| bad("INTNETADR", t))?;
    }
    Ok(Ipv4Addr::from(b))
}

/// A subnet mask: contiguous ones; `*HOST` is `255.255.255.255`.
pub fn parse_mask(s: &str) -> Result<Ipv4Addr> {
    let t = unquote(s);
    if t.eq_ignore_ascii_case("*HOST") {
        return Ok(Ipv4Addr::BROADCAST);
    }
    let m = parse_v4(t).map_err(|_| bad("SUBNETMASK", t))?;
    mask_prefix(m).ok_or_else(|| bad("SUBNETMASK", t))?;
    Ok(m)
}

/// The prefix length of a contiguous mask.
pub fn mask_prefix(m: Ipv4Addr) -> Option<u8> {
    let v = u32::from(m);
    let ones = v.leading_ones();
    (v.checked_shl(ones).unwrap_or(0) == 0).then(|| u8::try_from(ones).unwrap_or(32))
}

/// The mask of a prefix length.
pub fn prefix_mask(prefix: u8) -> Ipv4Addr {
    let p = u32::from(prefix.min(32));
    Ipv4Addr::from(if p == 0 { 0 } else { u32::MAX << (32 - p) })
}

/// The network of `ip/prefix`.
pub fn network(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => IpAddr::V4(Ipv4Addr::from(u32::from(v4) & u32::from(prefix_mask(prefix)))),
        IpAddr::V6(v6) => {
            let p = u32::from(prefix.min(128));
            let mask = if p == 0 { 0 } else { u128::MAX << (128 - p) };
            IpAddr::V6(Ipv6Addr::from(u128::from(v6) & mask))
        }
    }
}

/// Whether `ip` is in `net/prefix`.
pub fn contains(net: IpAddr, prefix: u8, ip: IpAddr) -> bool {
    net.is_ipv4() == ip.is_ipv4() && network(net, prefix) == network(ip, prefix)
}

/// The `TCP1D08` refusal for a value.
pub fn bad(kwd: &str, value: &str) -> Error {
    Error::new(
        msg::BAD_VALUE,
        format!("{value} is not a valid value for {kwd}."),
    )
}

/// The address as a 32-bit binary field holds it (`BININTADR`).
pub fn v4_bits(ip: Ipv4Addr) -> i64 {
    i64::from(u32::from(ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_and_prefixes() {
        assert_eq!(mask_prefix(Ipv4Addr::new(255, 255, 255, 0)), Some(24));
        assert_eq!(mask_prefix(Ipv4Addr::new(255, 0, 255, 0)), None);
        assert_eq!(mask_prefix(Ipv4Addr::UNSPECIFIED), Some(0));
        assert_eq!(prefix_mask(8), Ipv4Addr::new(255, 0, 0, 0));
        assert_eq!(parse_mask("*HOST").unwrap(), Ipv4Addr::BROADCAST);
        assert!(parse_mask("255.0.255.0").is_err());
    }

    #[test]
    fn addresses() {
        assert_eq!(
            IfcAddr::parse("'010.000.002.015'").unwrap(),
            IfcAddr::Ip("10.0.2.15".parse().unwrap())
        );
        assert_eq!(IfcAddr::parse("*ip4dhcp").unwrap(), IfcAddr::Dhcp4);
        assert!(IfcAddr::parse("fe80::1").unwrap().is_v6());
        assert!(parse_ip("1.2.3").is_err());
        assert!(parse_ip("256.1.1.1").is_err());
        let net: IpAddr = "185.113.5.0".parse().unwrap();
        assert!(contains(net, 24, "185.113.5.134".parse().unwrap()));
        assert!(!contains(net, 24, "185.113.6.1".parse().unwrap()));
    }
}
