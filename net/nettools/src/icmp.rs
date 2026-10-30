// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ICMP echo request and reply (RFC 792, RFC 4443), as an ICMP datagram
//! socket sends and receives them: the eight-byte header and the data,
//! without the IP header. The kernel sets the identifier to the
//! socket's and fills the IPv6 checksum; the IPv4 checksum is computed
//! here as well, so the packet stands on its own.

/// `ICMPv4` echo reply.
pub const ECHO_REPLY: u8 = 0;
/// `ICMPv4` destination unreachable.
pub const DEST_UNREACH: u8 = 3;
/// `ICMPv4` echo request.
pub const ECHO_REQUEST: u8 = 8;
/// `ICMPv4` time exceeded.
pub const TIME_EXCEEDED: u8 = 11;
/// `ICMPv6` destination unreachable.
pub const V6_DEST_UNREACH: u8 = 1;
/// `ICMPv6` time exceeded.
pub const V6_TIME_EXCEEDED: u8 = 3;
/// `ICMPv6` echo request.
pub const V6_ECHO_REQUEST: u8 = 128;
/// `ICMPv6` echo reply.
pub const V6_ECHO_REPLY: u8 = 129;
/// The header's length.
pub const HEADER: usize = 8;

/// The Internet checksum (RFC 1071) of `data`.
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let (pairs, rest) = data.as_chunks::<2>();
    for c in pairs {
        sum += u32::from(u16::from_be_bytes(*c));
    }
    if let [last] = rest {
        sum += u32::from(*last) << 8;
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !u16::try_from(sum).unwrap_or(u16::MAX)
}

/// An echo request or reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Echo {
    /// `ICMPv6` rather than `ICMPv4`.
    pub v6: bool,
    /// A reply rather than a request.
    pub reply: bool,
    /// The identifier.
    pub id: u16,
    /// The sequence number.
    pub seq: u16,
    /// The data.
    pub data: Vec<u8>,
}

impl Echo {
    /// A request carrying `len` bytes of data: a pattern that counts up
    /// from the sequence number, so a reply's data can be checked.
    pub fn request(v6: bool, id: u16, seq: u16, len: usize) -> Self {
        let start = seq.to_be_bytes()[1];
        let data = (0..len).map(|i| i.to_le_bytes()[0].wrapping_add(start)).collect();
        Self { v6, reply: false, id, seq, data }
    }

    /// The type byte.
    pub fn kind(&self) -> u8 {
        match (self.v6, self.reply) {
            (false, false) => ECHO_REQUEST,
            (false, true) => ECHO_REPLY,
            (true, false) => V6_ECHO_REQUEST,
            (true, true) => V6_ECHO_REPLY,
        }
    }

    /// The packet: header and data, the IPv4 checksum filled (zero for
    /// IPv6, whose checksum covers a pseudo-header the kernel knows).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(HEADER + self.data.len());
        b.push(self.kind());
        b.push(0);
        b.extend_from_slice(&[0, 0]);
        b.extend_from_slice(&self.id.to_be_bytes());
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.extend_from_slice(&self.data);
        if !self.v6 {
            let c = checksum(&b);
            b[2..4].copy_from_slice(&c.to_be_bytes());
        }
        b
    }

    /// An echo packet, request or reply; `None` for anything else or a
    /// short one. An IPv4 packet must have a good checksum.
    pub fn decode(b: &[u8], v6: bool) -> Option<Self> {
        if b.len() < HEADER || b[1] != 0 {
            return None;
        }
        let reply = match (v6, b[0]) {
            (false, ECHO_REPLY) | (true, V6_ECHO_REPLY) => true,
            (false, ECHO_REQUEST) | (true, V6_ECHO_REQUEST) => false,
            _ => return None,
        };
        if !v6 && checksum(b) != 0 {
            return None;
        }
        Some(Self {
            v6,
            reply,
            id: u16::from_be_bytes([b[4], b[5]]),
            seq: u16::from_be_bytes([b[6], b[7]]),
            data: b[HEADER..].to_vec(),
        })
    }

    /// The reply a host would send to this request.
    #[must_use]
    pub fn answer(&self) -> Self {
        Self { reply: true, ..self.clone() }
    }
}

/// What an ICMP error says about a probe, by type and code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Time exceeded in transit: a router on the way.
    TimeExceeded,
    /// The port is unreachable: the destination answered a UDP probe.
    PortUnreachable,
    /// Unreachable otherwise; the letter traceroute marks it with
    /// (`N` network, `H` host, `P` protocol, `F` fragmentation needed,
    /// `X` administratively prohibited, `!` anything else).
    Unreachable(char),
    /// Another error.
    Other,
}

/// An ICMP error's meaning.
pub fn error_of(v6: bool, kind: u8, code: u8) -> Error {
    if v6 {
        return match (kind, code) {
            (V6_TIME_EXCEEDED, _) => Error::TimeExceeded,
            (V6_DEST_UNREACH, 4) => Error::PortUnreachable,
            (V6_DEST_UNREACH, 0) => Error::Unreachable('N'),
            (V6_DEST_UNREACH, 1) => Error::Unreachable('X'),
            (V6_DEST_UNREACH, 3) => Error::Unreachable('H'),
            (V6_DEST_UNREACH, _) => Error::Unreachable('!'),
            _ => Error::Other,
        };
    }
    match (kind, code) {
        (TIME_EXCEEDED, _) => Error::TimeExceeded,
        (DEST_UNREACH, 3) => Error::PortUnreachable,
        (DEST_UNREACH, 0) => Error::Unreachable('N'),
        (DEST_UNREACH, 1) => Error::Unreachable('H'),
        (DEST_UNREACH, 2) => Error::Unreachable('P'),
        (DEST_UNREACH, 4) => Error::Unreachable('F'),
        (DEST_UNREACH, 9 | 10 | 13) => Error::Unreachable('X'),
        (DEST_UNREACH, _) => Error::Unreachable('!'),
        _ => Error::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_rfc1071_example() {
        // RFC 1071 section 3: the sum of these words is 0xddf2.
        let data = [0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7];
        assert_eq!(checksum(&data), !0xddf2);
        // An odd length pads with a zero byte.
        assert_eq!(checksum(&[0xab]), !0xab00);
    }

    #[test]
    fn v4_request_round_trips_and_checks() {
        let e = Echo::request(false, 0x1234, 7, 56);
        let b = e.encode();
        assert_eq!(b.len(), 64);
        assert_eq!(&b[..2], &[8, 0]);
        assert_eq!(&b[4..8], &[0x12, 0x34, 0, 7]);
        assert_eq!(checksum(&b), 0, "a good packet sums to zero");
        assert_eq!(Echo::decode(&b, false), Some(e.clone()));
        let mut bad = b.clone();
        bad[10] ^= 1;
        assert_eq!(Echo::decode(&bad, false), None);
        let r = e.answer().encode();
        assert_eq!(r[0], ECHO_REPLY);
        let back = Echo::decode(&r, false).unwrap();
        assert!(back.reply);
        assert_eq!(back.data, e.data);
    }

    #[test]
    fn v6_leaves_the_checksum_to_the_kernel() {
        let e = Echo::request(true, 1, 2, 4);
        let b = e.encode();
        assert_eq!(b, vec![128, 0, 0, 0, 0, 1, 0, 2, 2, 3, 4, 5]);
        assert_eq!(Echo::decode(&b, true).unwrap().seq, 2);
        assert_eq!(Echo::decode(&b, false), None, "type 128 is not ICMPv4");
        assert_eq!(Echo::decode(&b[..7], true), None);
    }

    #[test]
    fn errors_by_type_and_code() {
        assert_eq!(error_of(false, 11, 0), Error::TimeExceeded);
        assert_eq!(error_of(false, 3, 3), Error::PortUnreachable);
        assert_eq!(error_of(false, 3, 1), Error::Unreachable('H'));
        assert_eq!(error_of(false, 3, 13), Error::Unreachable('X'));
        assert_eq!(error_of(true, 3, 0), Error::TimeExceeded);
        assert_eq!(error_of(true, 1, 4), Error::PortUnreachable);
        assert_eq!(error_of(false, 5, 0), Error::Other);
    }
}
