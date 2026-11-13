// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Universal addresses (RFC 5665 section 5.2.3): `h1.h2.h3.h4.p1.p2`
//! for IPv4 and `<IPv6 text>.p1.p2` for IPv6 (the oracle writes
//! `::.0.111`); netids; and the transport addresses of `UADDR2TADDR`
//! and `TADDR2UADDR` (a `sockaddr_in` or `sockaddr_in6` as Linux on
//! `x86_64` lays it out).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6};

use crate::types::rpcb::{IPPROTO_TCP, IPPROTO_UDP, NC_TPI_CLTS, NC_TPI_COTS_ORD};

/// `addr` as a universal address.
pub fn to_uaddr(addr: SocketAddr) -> String {
    let p = addr.port();
    format!("{}.{}.{}", addr.ip(), p >> 8, p & 0xff)
}

/// A universal address as a socket address.
pub fn from_uaddr(s: &str) -> Option<SocketAddr> {
    let mut parts = s.rsplitn(3, '.');
    let lo: u16 = parts.next()?.parse().ok().filter(|v| *v <= 255)?;
    let hi: u16 = parts.next()?.parse().ok().filter(|v| *v <= 255)?;
    let ip: IpAddr = parts.next()?.parse().ok()?;
    Some(SocketAddr::new(ip, (hi << 8) | lo))
}

/// The port of a universal address.
pub fn port_of(s: &str) -> Option<u16> {
    from_uaddr(s).map(|a| a.port())
}

/// A netid the binder knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Netid {
    /// `tcp`.
    Tcp,
    /// `udp`.
    Udp,
    /// `tcp6`.
    Tcp6,
    /// `udp6`.
    Udp6,
}

impl Netid {
    /// The netid named `s`.
    pub fn parse(s: &[u8]) -> Option<Self> {
        Some(match s {
            b"tcp" => Self::Tcp,
            b"udp" => Self::Udp,
            b"tcp6" => Self::Tcp6,
            b"udp6" => Self::Udp6,
            _ => return None,
        })
    }

    /// The name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Tcp6 => "tcp6",
            Self::Udp6 => "udp6",
        }
    }

    /// The IP protocol number (portmap's `prot`).
    pub fn protocol(self) -> u32 {
        match self {
            Self::Tcp | Self::Tcp6 => IPPROTO_TCP,
            Self::Udp | Self::Udp6 => IPPROTO_UDP,
        }
    }

    /// The IPv4 netid of portmap's `prot`.
    pub fn of_protocol(prot: u32) -> Option<Self> {
        match prot {
            IPPROTO_TCP => Some(Self::Tcp),
            IPPROTO_UDP => Some(Self::Udp),
            _ => None,
        }
    }

    /// The transport semantics (`NC_TPI_*`).
    pub fn semantics(self) -> u32 {
        match self {
            Self::Tcp | Self::Tcp6 => NC_TPI_COTS_ORD,
            Self::Udp | Self::Udp6 => NC_TPI_CLTS,
        }
    }

    /// The protocol family name.
    pub fn family(self) -> &'static str {
        match self {
            Self::Tcp | Self::Udp => "inet",
            Self::Tcp6 | Self::Udp6 => "inet6",
        }
    }

    /// The protocol name.
    pub fn proto(self) -> &'static str {
        match self {
            Self::Tcp | Self::Tcp6 => "tcp",
            Self::Udp | Self::Udp6 => "udp",
        }
    }
}

const AF_INET: u16 = 2;
const AF_INET6: u16 = 10;

/// A universal address as a transport address (`sockaddr_in` or
/// `sockaddr_in6`, the family in the machine's order).
pub fn to_taddr(s: &str) -> Option<Vec<u8>> {
    let a = from_uaddr(s)?;
    let mut b = Vec::with_capacity(28);
    match a {
        SocketAddr::V4(v) => {
            b.extend_from_slice(&AF_INET.to_le_bytes());
            b.extend_from_slice(&v.port().to_be_bytes());
            b.extend_from_slice(&v.ip().octets());
            b.extend_from_slice(&[0; 8]);
        }
        SocketAddr::V6(v) => {
            b.extend_from_slice(&AF_INET6.to_le_bytes());
            b.extend_from_slice(&v.port().to_be_bytes());
            b.extend_from_slice(&v.flowinfo().to_be_bytes());
            b.extend_from_slice(&v.ip().octets());
            b.extend_from_slice(&v.scope_id().to_le_bytes());
        }
    }
    Some(b)
}

/// A transport address as a universal address.
pub fn from_taddr(b: &[u8]) -> Option<String> {
    let fam = u16::from_le_bytes([*b.first()?, *b.get(1)?]);
    let port = u16::from_be_bytes([*b.get(2)?, *b.get(3)?]);
    let ip = match (fam, b.len()) {
        (AF_INET, 16) => IpAddr::V4(Ipv4Addr::new(
            *b.get(4)?,
            *b.get(5)?,
            *b.get(6)?,
            *b.get(7)?,
        )),
        (AF_INET6, 28) => {
            let o: [u8; 16] = b.get(8..24)?.try_into().ok()?;
            let ip = Ipv6Addr::from(o);
            return Some(to_uaddr(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0))));
        }
        _ => return None,
    };
    Some(to_uaddr(SocketAddr::new(ip, port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn universal_addresses() {
        let a: SocketAddr = "10.0.2.15:2049".parse().unwrap();
        assert_eq!(to_uaddr(a), "10.0.2.15.8.1");
        assert_eq!(from_uaddr("10.0.2.15.8.1"), Some(a));
        // The oracle's form.
        assert_eq!(from_uaddr("::.0.111"), Some("[::]:111".parse().unwrap()));
        assert_eq!(to_uaddr("[::]:111".parse().unwrap()), "::.0.111");
        assert_eq!(from_uaddr("fe80::1.225.223").map(|a| a.port()), Some(57823));
        for bad in [
            "",
            "1.2.3.4",
            "1.2.3.4.256.1",
            "x.0.1",
            "1.2.3.4.1.-1",
            "::.1",
        ] {
            assert_eq!(from_uaddr(bad), None, "{bad}");
        }
    }

    #[test]
    fn transport_addresses() {
        let t = to_taddr("127.0.0.1.0.111").unwrap();
        assert_eq!(t, [2, 0, 0, 111, 127, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(from_taddr(&t).as_deref(), Some("127.0.0.1.0.111"));
        let t6 = to_taddr("::1.8.1").unwrap();
        assert_eq!(t6.len(), 28);
        assert_eq!(from_taddr(&t6).as_deref(), Some("::1.8.1"));
        assert_eq!(from_taddr(&[2, 0]), None);
        assert_eq!(from_taddr(&[9; 16]), None);
    }

    #[test]
    fn netids() {
        assert_eq!(Netid::parse(b"tcp6"), Some(Netid::Tcp6));
        assert_eq!(Netid::parse(b"tcp_ord"), None);
        assert_eq!(Netid::of_protocol(17), Some(Netid::Udp));
        assert_eq!(Netid::Udp6.family(), "inet6");
    }
}
