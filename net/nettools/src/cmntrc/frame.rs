// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The headers of a traced Ethernet frame: the MAC header (with an
//! 802.1Q tag), ARP, IPv4, IPv6, and TCP, UDP or ICMP over IP. Only
//! what the report prints is decoded; a frame too short for a header
//! simply stops there.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// `EtherType` IPv4.
pub const ETH_IP: u16 = 0x0800;
/// `EtherType` ARP.
pub const ETH_ARP: u16 = 0x0806;
/// `EtherType` IPv6.
pub const ETH_IPV6: u16 = 0x86dd;
/// `EtherType` 802.1Q.
pub const ETH_VLAN: u16 = 0x8100;

/// An ARP packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arp {
    /// 1 request, 2 reply.
    pub op: u16,
    /// Sender hardware address.
    pub sha: [u8; 6],
    /// Sender protocol address.
    pub spa: Ipv4Addr,
    /// Target hardware address.
    pub tha: [u8; 6],
    /// Target protocol address.
    pub tpa: Ipv4Addr,
}

/// An IP header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ip {
    /// Source.
    pub src: IpAddr,
    /// Destination.
    pub dst: IpAddr,
    /// The transport protocol (`next header` for IPv6).
    pub proto: u8,
    /// TTL or hop limit.
    pub ttl: u8,
    /// Type of service, traffic class for IPv6.
    pub tos: u8,
    /// Total length (payload length plus 40 for IPv6).
    pub len: u16,
    /// Datagram identifier (IPv4), flow label (IPv6).
    pub id: u32,
    /// IPv4 flags and fragment offset: don't fragment, more fragments,
    /// offset in bytes.
    pub frag: (bool, bool, u16),
    /// The header's bytes.
    pub header: Vec<u8>,
}

/// A transport header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum L4 {
    /// TCP.
    Tcp {
        /// Source port.
        src: u16,
        /// Destination port.
        dst: u16,
        /// Sequence number.
        seq: u32,
        /// Acknowledgment number.
        ack: u32,
        /// The flag bits (`FIN` is 0x01 .. `CWR` 0x80).
        flags: u8,
        /// Window.
        window: u16,
        /// The header's bytes.
        header: Vec<u8>,
    },
    /// UDP.
    Udp {
        /// Source port.
        src: u16,
        /// Destination port.
        dst: u16,
        /// Length.
        len: u16,
    },
    /// ICMP, version 4 or 6.
    Icmp {
        /// Type.
        kind: u8,
        /// Code.
        code: u8,
    },
}

/// What a frame holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Destination MAC.
    pub dst: [u8; 6],
    /// Source MAC.
    pub src: [u8; 6],
    /// The `EtherType` after any VLAN tag (or the 802.3 length).
    pub ethertype: u16,
    /// The VLAN tag's identifier.
    pub vlan: Option<u16>,
    /// ARP.
    pub arp: Option<Arp>,
    /// IP.
    pub ip: Option<Ip>,
    /// The transport header.
    pub l4: Option<L4>,
    /// Where the transport payload starts.
    pub payload: usize,
}

impl Frame {
    /// Sent to the broadcast or a multicast address.
    pub fn broadcast(&self) -> bool {
        self.dst[0] & 1 == 1
    }

    /// The transport ports, when TCP or UDP.
    pub fn ports(&self) -> Option<(u16, u16)> {
        match self.l4 {
            Some(L4::Tcp { src, dst, .. } | L4::Udp { src, dst, .. }) => Some((src, dst)),
            _ => None,
        }
    }
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn mac(b: &[u8], at: usize) -> Option<[u8; 6]> {
    b.get(at..at + 6)?.try_into().ok()
}

/// Decode a frame; `None` when it is shorter than a MAC header.
pub fn parse(b: &[u8]) -> Option<Frame> {
    let mut f = Frame {
        dst: mac(b, 0)?,
        src: mac(b, 6)?,
        ethertype: be16(b, 12)?,
        vlan: None,
        arp: None,
        ip: None,
        l4: None,
        payload: 14,
    };
    let mut at = 14;
    if f.ethertype == ETH_VLAN {
        if let (Some(tci), Some(t)) = (be16(b, 14), be16(b, 16)) {
            f.vlan = Some(tci & 0x0fff);
            f.ethertype = t;
            at = 18;
        } else {
            return Some(f);
        }
    }
    f.payload = at;
    match f.ethertype {
        ETH_ARP => f.arp = arp(&b[at..]),
        ETH_IP => {
            if let Some((ip, hl)) = ipv4(&b[at..]) {
                at += hl;
                f.payload = at;
                transport(&mut f, b, at, ip.proto);
                f.ip = Some(ip);
            }
        }
        ETH_IPV6 => {
            if let Some(ip) = ipv6(&b[at..]) {
                at += 40;
                f.payload = at;
                transport(&mut f, b, at, ip.proto);
                f.ip = Some(ip);
            }
        }
        _ => {}
    }
    Some(f)
}

fn arp(b: &[u8]) -> Option<Arp> {
    // Ethernet and IPv4 only: hardware length 6, protocol length 4.
    if be16(b, 0)? != 1 || be16(b, 2)? != ETH_IP || *b.get(4)? != 6 || *b.get(5)? != 4 {
        return None;
    }
    let ip = |at: usize| -> Option<Ipv4Addr> { Some(Ipv4Addr::from(be32(b, at)?)) };
    Some(Arp {
        op: be16(b, 6)?,
        sha: mac(b, 8)?,
        spa: ip(14)?,
        tha: mac(b, 18)?,
        tpa: ip(24)?,
    })
}

fn ipv4(b: &[u8]) -> Option<(Ip, usize)> {
    let v = *b.first()?;
    if v >> 4 != 4 {
        return None;
    }
    let hl = usize::from(v & 0x0f) * 4;
    if hl < 20 || b.len() < hl {
        return None;
    }
    let ff = be16(b, 6)?;
    Some((
        Ip {
            src: IpAddr::V4(Ipv4Addr::from(be32(b, 12)?)),
            dst: IpAddr::V4(Ipv4Addr::from(be32(b, 16)?)),
            proto: b[9],
            ttl: b[8],
            tos: b[1],
            len: be16(b, 2)?,
            id: u32::from(be16(b, 4)?),
            frag: (ff & 0x4000 != 0, ff & 0x2000 != 0, (ff & 0x1fff) * 8),
            header: b[..hl].to_vec(),
        },
        hl,
    ))
}

fn ipv6(b: &[u8]) -> Option<Ip> {
    if b.len() < 40 || b[0] >> 4 != 6 {
        return None;
    }
    let first = be32(b, 0)?;
    let addr = |at: usize| -> Option<IpAddr> {
        let o: [u8; 16] = b.get(at..at + 16)?.try_into().ok()?;
        Some(IpAddr::V6(Ipv6Addr::from(o)))
    };
    Some(Ip {
        src: addr(8)?,
        dst: addr(24)?,
        proto: b[6],
        ttl: b[7],
        tos: ((first >> 20) & 0xff).to_be_bytes()[3],
        len: be16(b, 4)?.saturating_add(40),
        id: first & 0x000f_ffff,
        frag: (false, false, 0),
        header: b[..40].to_vec(),
    })
}

fn transport(f: &mut Frame, b: &[u8], at: usize, proto: u8) {
    let t = &b[at.min(b.len())..];
    f.l4 = match proto {
        6 => (|| {
            let hl = usize::from(*t.get(12)? >> 4) * 4;
            if hl < 20 || t.len() < hl {
                return None;
            }
            f.payload = at + hl;
            Some(L4::Tcp {
                src: be16(t, 0)?,
                dst: be16(t, 2)?,
                seq: be32(t, 4)?,
                ack: be32(t, 8)?,
                flags: t[13],
                window: be16(t, 14)?,
                header: t[..hl].to_vec(),
            })
        })(),
        17 => (|| {
            f.payload = at + 8;
            Some(L4::Udp {
                src: be16(t, 0)?,
                dst: be16(t, 2)?,
                len: be16(t, 4)?,
            })
        })(),
        1 | 58 => (|| {
            f.payload = at + 4;
            Some(L4::Icmp { kind: *t.first()?, code: *t.get(1)? })
        })(),
        _ => None,
    };
}

/// The names of the TCP flag bits set, in the order of the bits.
pub fn tcp_flags(flags: u8) -> Vec<&'static str> {
    const NAMES: [&str; 8] = ["FIN", "SYN", "RST", "PSH", "ACK", "URG", "ECE", "CWR"];
    (0..8).filter(|i| flags & (1 << i) != 0).map(|i| NAMES[i]).collect()
}

/// Frames for tests: an Ethernet header and an IPv4 header around a
/// TCP or UDP header.
#[cfg(test)]
pub(crate) fn sample(proto: u8, src: [u8; 4], dst: [u8; 4], sport: u16, dport: u16) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0x52, 0x55, 0x0a, 0, 2, 2]);
    b.extend_from_slice(&[0x52, 0x54, 0, 0x12, 0x34, 0x56]);
    b.extend_from_slice(&ETH_IP.to_be_bytes());
    let l4len: u16 = if proto == 6 { 20 } else { 8 };
    let total = 20 + l4len + 4;
    b.extend_from_slice(&[0x45, 0, 0, 0, 0x1a, 0x2b, 0x40, 0, 64, proto, 0, 0]);
    b[16..18].copy_from_slice(&total.to_be_bytes());
    b.extend_from_slice(&src);
    b.extend_from_slice(&dst);
    b.extend_from_slice(&sport.to_be_bytes());
    b.extend_from_slice(&dport.to_be_bytes());
    if proto == 6 {
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&2000u32.to_be_bytes());
        b.extend_from_slice(&[0x50, 0x18, 0xff, 0xff, 0, 0, 0, 0]);
    } else {
        b.extend_from_slice(&(l4len + 4).to_be_bytes());
        b.extend_from_slice(&[0, 0]);
    }
    b.extend_from_slice(b"ping");
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcp_over_ipv4() {
        let b = sample(6, [10, 0, 2, 15], [10, 0, 2, 2], 23, 40000);
        let f = parse(&b).unwrap();
        assert_eq!(f.ethertype, ETH_IP);
        assert!(!f.broadcast());
        let ip = f.ip.as_ref().unwrap();
        assert_eq!(ip.src, "10.0.2.15".parse::<IpAddr>().unwrap());
        assert_eq!(ip.ttl, 64);
        assert_eq!(ip.id, 0x1a2b);
        assert_eq!(ip.frag, (true, false, 0));
        assert_eq!(ip.len, 44);
        match &f.l4 {
            Some(L4::Tcp { src, dst, seq, ack, flags, window, .. }) => {
                assert_eq!((*src, *dst, *seq, *ack, *window), (23, 40000, 1000, 2000, 0xffff));
                assert_eq!(tcp_flags(*flags), ["PSH", "ACK"]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(&b[f.payload..], b"ping");
        assert_eq!(f.ports(), Some((23, 40000)));
    }

    #[test]
    fn udp_and_short_frames() {
        let b = sample(17, [10, 0, 2, 15], [10, 0, 2, 3], 5353, 53);
        let f = parse(&b).unwrap();
        assert_eq!(f.l4, Some(L4::Udp { src: 5353, dst: 53, len: 12 }));
        assert_eq!(&b[f.payload..], b"ping");
        assert_eq!(parse(&b[..13]), None);
        let cut = parse(&b[..20]).unwrap();
        assert_eq!(cut.ip, None, "the IP header is cut");
    }

    #[test]
    fn arp_request_is_broadcast() {
        let mut b = vec![0xff; 6];
        b.extend_from_slice(&[0x52, 0x54, 0, 0x12, 0x34, 0x56]);
        b.extend_from_slice(&ETH_ARP.to_be_bytes());
        b.extend_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]);
        b.extend_from_slice(&[0x52, 0x54, 0, 0x12, 0x34, 0x56, 10, 0, 2, 15]);
        b.extend_from_slice(&[0, 0, 0, 0, 0, 0, 10, 0, 2, 2]);
        let f = parse(&b).unwrap();
        assert!(f.broadcast());
        let a = f.arp.unwrap();
        assert_eq!(a.op, 1);
        assert_eq!(a.tpa, Ipv4Addr::new(10, 0, 2, 2));
    }

    #[test]
    fn vlan_tag_and_ipv6() {
        let mut b = vec![0x33, 0x33, 0, 0, 0, 1, 0x52, 0x54, 0, 0x12, 0x34, 0x56];
        b.extend_from_slice(&ETH_VLAN.to_be_bytes());
        b.extend_from_slice(&[0x00, 0x0a]);
        b.extend_from_slice(&ETH_IPV6.to_be_bytes());
        b.extend_from_slice(&[0x60, 0, 0, 0, 0, 8, 58, 255]);
        b.extend_from_slice(&"fe80::1".parse::<Ipv6Addr>().unwrap().octets());
        b.extend_from_slice(&"ff02::1".parse::<Ipv6Addr>().unwrap().octets());
        b.extend_from_slice(&[128, 0, 0, 0, 0, 1, 0, 1]);
        let f = parse(&b).unwrap();
        assert_eq!(f.vlan, Some(10));
        let ip = f.ip.as_ref().unwrap();
        assert_eq!(ip.proto, 58);
        assert_eq!(ip.len, 48);
        assert_eq!(f.l4, Some(L4::Icmp { kind: 128, code: 0 }));
    }
}
