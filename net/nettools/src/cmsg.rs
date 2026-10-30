// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Ancillary data from `recvmsg`, parsed from the bytes of the control
//! buffer as Linux lays them out on `x86_64`: a `cmsghdr` of 16 bytes
//! (`cmsg_len` 8, level 4, type 4), the data, padding to 8. What the
//! tools read: the received TTL or hop limit, and `IP_RECVERR`'s
//! `sock_extended_err` with the address of the node that sent the ICMP
//! error.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// `SOL_IP`.
pub const SOL_IP: i32 = 0;
/// `SOL_IPV6`.
pub const SOL_IPV6: i32 = 41;
/// `IP_TTL` (a received TTL with `IP_RECVTTL`).
pub const IP_TTL: i32 = 2;
/// `IP_RECVERR`.
pub const IP_RECVERR: i32 = 11;
/// `IPV6_RECVERR`.
pub const IPV6_RECVERR: i32 = 25;
/// `IPV6_HOPLIMIT` (a received hop limit with `IPV6_RECVHOPLIMIT`).
pub const IPV6_HOPLIMIT: i32 = 52;
/// `SO_EE_ORIGIN_LOCAL`.
pub const ORIGIN_LOCAL: u8 = 1;
/// `SO_EE_ORIGIN_ICMP`.
pub const ORIGIN_ICMP: u8 = 2;
/// `SO_EE_ORIGIN_ICMP6`.
pub const ORIGIN_ICMP6: u8 = 3;

const HDR: usize = 16;
const ERR_LEN: usize = 16;

fn align(n: usize) -> usize {
    (n + 7) & !7
}

fn u32_ne(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The messages of a control buffer: `(level, type, data)`.
pub fn messages(ctrl: &[u8]) -> Vec<(i32, i32, &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + HDR <= ctrl.len() {
        let Some(len) = ctrl
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_ne_bytes)
            .and_then(|l| usize::try_from(l).ok())
        else {
            break;
        };
        if len < HDR || at + len > ctrl.len() {
            break;
        }
        let level = u32_ne(ctrl, at + 8).map_or(-1, u32::cast_signed);
        let kind = u32_ne(ctrl, at + 12).map_or(-1, u32::cast_signed);
        out.push((level, kind, &ctrl[at + HDR..at + len]));
        at += align(len);
    }
    out
}

/// The TTL or hop limit of a received packet, when the socket asked
/// for it.
pub fn ttl(ctrl: &[u8]) -> Option<u8> {
    messages(ctrl).into_iter().find_map(|(level, kind, data)| {
        let ok = (level == SOL_IP && kind == IP_TTL) || (level == SOL_IPV6 && kind == IPV6_HOPLIMIT);
        if !ok {
            return None;
        }
        u32_ne(data, 0).and_then(|v| u8::try_from(v).ok())
    })
}

/// `struct sock_extended_err` and the offender address after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtendedErr {
    /// `ee_errno`.
    pub errno: u32,
    /// `ee_origin`.
    pub origin: u8,
    /// `ee_type`: the ICMP type.
    pub kind: u8,
    /// `ee_code`: the ICMP code.
    pub code: u8,
    /// `ee_info` (the MTU for "fragmentation needed").
    pub info: u32,
    /// The node that sent the error.
    pub offender: Option<IpAddr>,
}

/// A `sockaddr_in` or `sockaddr_in6`'s address.
pub fn sockaddr_ip(b: &[u8]) -> Option<IpAddr> {
    let family = u16::from_ne_bytes(b.get(0..2)?.try_into().ok()?);
    match family {
        2 => {
            let o: [u8; 4] = b.get(4..8)?.try_into().ok()?;
            Some(IpAddr::V4(Ipv4Addr::from(o)))
        }
        10 => {
            let o: [u8; 16] = b.get(8..24)?.try_into().ok()?;
            Some(IpAddr::V6(Ipv6Addr::from(o)))
        }
        _ => None,
    }
}

/// The extended error of an error-queue read.
pub fn extended_err(ctrl: &[u8]) -> Option<ExtendedErr> {
    messages(ctrl).into_iter().find_map(|(level, kind, d)| {
        let ok = (level == SOL_IP && kind == IP_RECVERR) || (level == SOL_IPV6 && kind == IPV6_RECVERR);
        if !ok || d.len() < ERR_LEN {
            return None;
        }
        Some(ExtendedErr {
            errno: u32_ne(d, 0)?,
            origin: d[4],
            kind: d[5],
            code: d[6],
            info: u32_ne(d, 8)?,
            offender: sockaddr_ip(&d[ERR_LEN..]).filter(|ip| !ip.is_unspecified()),
        })
    })
}

/// A control buffer holding one message, as the kernel would write it
/// (for tests and fakes).
pub fn build(level: i32, kind: i32, data: &[u8]) -> Vec<u8> {
    let len = HDR + data.len();
    let mut b = Vec::with_capacity(align(len));
    b.extend_from_slice(&(len as u64).to_ne_bytes());
    b.extend_from_slice(&level.to_ne_bytes());
    b.extend_from_slice(&kind.to_ne_bytes());
    b.extend_from_slice(data);
    b.resize(align(len), 0);
    b
}

/// The data of an `IP_RECVERR` message (for tests and fakes).
pub fn err_data(e: &ExtendedErr) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&e.errno.to_ne_bytes());
    d.extend_from_slice(&[e.origin, e.kind, e.code, 0]);
    d.extend_from_slice(&e.info.to_ne_bytes());
    d.extend_from_slice(&0u32.to_ne_bytes());
    match e.offender {
        Some(IpAddr::V4(a)) => {
            d.extend_from_slice(&2u16.to_ne_bytes());
            d.extend_from_slice(&[0, 0]);
            d.extend_from_slice(&a.octets());
            d.extend_from_slice(&[0; 8]);
        }
        Some(IpAddr::V6(a)) => {
            d.extend_from_slice(&10u16.to_ne_bytes());
            d.extend_from_slice(&[0; 6]);
            d.extend_from_slice(&a.octets());
            d.extend_from_slice(&[0; 4]);
        }
        None => d.extend_from_slice(&[0; 16]),
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_message() {
        let mut b = build(SOL_IP, IP_TTL, &64u32.to_ne_bytes());
        assert_eq!(b.len(), 24, "16 header + 4 data, padded to 8");
        assert_eq!(ttl(&b), Some(64));
        b.extend(build(SOL_IPV6, IPV6_HOPLIMIT, &255u32.to_ne_bytes()));
        assert_eq!(messages(&b).len(), 2);
        assert_eq!(ttl(&build(SOL_IPV6, IPV6_HOPLIMIT, &255u32.to_ne_bytes())), Some(255));
        assert_eq!(ttl(&build(SOL_IP, 99, &[1, 0, 0, 0])), None);
    }

    #[test]
    fn extended_error_with_offender() {
        let e = ExtendedErr {
            errno: 113,
            origin: ORIGIN_ICMP,
            kind: 11,
            code: 0,
            info: 0,
            offender: Some("10.0.2.2".parse().unwrap()),
        };
        let b = build(SOL_IP, IP_RECVERR, &err_data(&e));
        assert_eq!(extended_err(&b), Some(e));
        let e6 = ExtendedErr {
            origin: ORIGIN_ICMP6,
            kind: 3,
            offender: Some("fe80::1".parse().unwrap()),
            ..e
        };
        let b = build(SOL_IPV6, IPV6_RECVERR, &err_data(&e6));
        assert_eq!(extended_err(&b), Some(e6));
    }

    #[test]
    fn truncated_buffers_stop() {
        let b = build(SOL_IP, IP_TTL, &64u32.to_ne_bytes());
        assert!(messages(&b[..10]).is_empty());
        assert!(messages(&b[..18]).is_empty(), "length past the end");
        assert_eq!(extended_err(&build(SOL_IP, IP_RECVERR, &[0; 8])), None);
    }
}
