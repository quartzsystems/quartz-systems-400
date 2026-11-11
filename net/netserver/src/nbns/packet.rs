// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Name service packets (RFC 1002 4.2): the header, questions and resource
//! records, with the label pointer RFC 1002 4.1 allows for a record's name.

use std::net::Ipv4Addr;

use super::NbnsError;
use crate::nbss::NbName;

/// The longest packet read or made (an Ethernet frame's UDP payload).
pub const MAX_PACKET: usize = 1472;
/// The most records of one section a packet may carry.
const MAX_RECORDS: u16 = 16;

/// `OPCODE` query.
pub const OP_QUERY: u8 = 0x0;
/// `OPCODE` registration.
pub const OP_REGISTRATION: u8 = 0x5;
/// `OPCODE` release.
pub const OP_RELEASE: u8 = 0x6;
/// `OPCODE` wait for acknowledgement.
pub const OP_WACK: u8 = 0x7;
/// `OPCODE` refresh.
pub const OP_REFRESH: u8 = 0x8;
/// `OPCODE` refresh as Windows sends it (RFC 1002 errata).
pub const OP_REFRESH_ALT: u8 = 0x9;
/// `OPCODE` multi-homed registration.
pub const OP_MULTIHOMED: u8 = 0xF;

/// `RCODE` name error: the name is not here.
pub const RCODE_NAM_ERR: u8 = 0x3;
/// `RCODE` active error: the name is owned by another node.
pub const RCODE_ACT_ERR: u8 = 0x6;
/// `RCODE` conflict: the name is in conflict (a name conflict demand).
pub const RCODE_CFT_ERR: u8 = 0x7;

/// `RR_TYPE` and `Q_TYPE` general name service resource record.
pub const TYPE_NB: u16 = 0x0020;
/// `RR_TYPE` and `Q_TYPE` node status resource record.
pub const TYPE_NBSTAT: u16 = 0x0021;
/// `RR_TYPE` null resource record.
pub const TYPE_NULL: u16 = 0x000A;
/// `RR_CLASS` and `Q_CLASS` Internet.
pub const CLASS_IN: u16 = 0x0001;

/// `NB_FLAGS` group name.
pub const NB_GROUP: u16 = 0x8000;
/// `NAME_FLAGS` group name (node status).
pub const NAME_GROUP: u16 = 0x8000;
/// `NAME_FLAGS` the name is active.
pub const NAME_ACTIVE: u16 = 0x0400;

const R: u16 = 0x8000;
const AA: u16 = 0x0400;
const TC: u16 = 0x0200;
const RD: u16 = 0x0100;
const RA: u16 = 0x0080;
const B: u16 = 0x0010;

/// The header's fields (RFC 1002 4.2.1.1) other than the counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // the header's flags
pub struct Header {
    /// `NAME_TRN_ID`.
    pub id: u16,
    /// A response (`R`).
    pub response: bool,
    /// `OPCODE`.
    pub opcode: u8,
    /// Authoritative answer (`AA`).
    pub aa: bool,
    /// Truncation (`TC`).
    pub tc: bool,
    /// Recursion desired (`RD`).
    pub rd: bool,
    /// Recursion available (`RA`).
    pub ra: bool,
    /// Broadcast (`B`).
    pub broadcast: bool,
    /// `RCODE`.
    pub rcode: u8,
}

impl Header {
    fn word(&self) -> u16 {
        let bit = |b: bool, v: u16| if b { v } else { 0 };
        bit(self.response, R)
            | (u16::from(self.opcode & 0xF) << 11)
            | bit(self.aa, AA)
            | bit(self.tc, TC)
            | bit(self.rd, RD)
            | bit(self.ra, RA)
            | bit(self.broadcast, B)
            | u16::from(self.rcode & 0xF)
    }

    fn from_word(id: u16, w: u16) -> Self {
        Self {
            id,
            response: w & R != 0,
            opcode: ((w >> 11) & 0xF) as u8,
            aa: w & AA != 0,
            tc: w & TC != 0,
            rd: w & RD != 0,
            ra: w & RA != 0,
            broadcast: w & B != 0,
            rcode: (w & 0xF) as u8,
        }
    }
}

/// A question entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// `QUESTION_NAME`.
    pub name: NbName,
    /// `QUESTION_TYPE`: [`TYPE_NB`] or [`TYPE_NBSTAT`].
    pub qtype: u16,
}

/// A resource record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// `RR_NAME`.
    pub name: NbName,
    /// `RR_TYPE`.
    pub rtype: u16,
    /// `TTL` in seconds.
    pub ttl: u32,
    /// `RDATA`.
    pub rdata: Vec<u8>,
}

impl Resource {
    /// An `NB` record for one address with `flags` (`NB_FLAGS`).
    #[must_use]
    pub fn nb(name: NbName, ttl: u32, flags: u16, addr: Ipv4Addr) -> Self {
        let mut rdata = flags.to_be_bytes().to_vec();
        rdata.extend_from_slice(&addr.octets());
        Self {
            name,
            rtype: TYPE_NB,
            ttl,
            rdata,
        }
    }

    /// The `(NB_FLAGS, NB_ADDRESS)` entries of an `NB` record.
    #[must_use]
    pub fn addresses(&self) -> Vec<(u16, Ipv4Addr)> {
        self.rdata
            .as_chunks::<6>()
            .0
            .iter()
            .map(|&[f0, f1, a, b, c, d]| (u16::from_be_bytes([f0, f1]), Ipv4Addr::new(a, b, c, d)))
            .collect()
    }
}

/// A name service packet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Packet {
    /// The header.
    pub header: Header,
    /// The question section.
    pub questions: Vec<Question>,
    /// The answer section.
    pub answers: Vec<Resource>,
    /// The authority section.
    pub authority: Vec<Resource>,
    /// The additional section.
    pub additional: Vec<Resource>,
}

/// Reads a name at `pos` of `b`: in full, or a label pointer (RFC 1002
/// 4.1) to a full name earlier in the packet. Returns it and its length.
fn read_name(b: &[u8], pos: usize) -> Result<(NbName, usize), NbnsError> {
    let first = *b.get(pos).ok_or(NbnsError::Truncated)?;
    if first & 0xC0 == 0xC0 {
        let lo = *b.get(pos + 1).ok_or(NbnsError::Truncated)?;
        let at = usize::from(u16::from_be_bytes([first & 0x3F, lo]));
        // Only backwards, and only to a full name.
        if at >= pos || b.get(at).is_none_or(|&x| x & 0xC0 != 0) {
            return Err(NbnsError::BadPointer);
        }
        let (name, _) = NbName::decode(&b[at..]).map_err(|_| NbnsError::BadName)?;
        return Ok((name, 2));
    }
    NbName::decode(&b[pos..]).map_err(|_| NbnsError::BadName)
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, NbnsError> {
    let s = b.get(at..at + 2).ok_or(NbnsError::Truncated)?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, NbnsError> {
    let s = b.get(at..at + 4).ok_or(NbnsError::Truncated)?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

impl Packet {
    /// Parses a packet.
    ///
    /// # Errors
    ///
    /// A short or malformed packet, more than 16 records in a section, or
    /// a packet longer than [`MAX_PACKET`].
    pub fn parse(b: &[u8]) -> Result<Self, NbnsError> {
        if b.len() > MAX_PACKET {
            return Err(NbnsError::TooLong);
        }
        let header = Header::from_word(u16_at(b, 0)?, u16_at(b, 2)?);
        let counts = [u16_at(b, 4)?, u16_at(b, 6)?, u16_at(b, 8)?, u16_at(b, 10)?];
        if counts.iter().any(|&n| n > MAX_RECORDS) {
            return Err(NbnsError::TooMany);
        }
        let mut pos = 12;
        let mut questions = Vec::new();
        for _ in 0..counts[0] {
            let (name, n) = read_name(b, pos)?;
            pos += n;
            let qtype = u16_at(b, pos)?;
            pos += 4;
            questions.push(Question { name, qtype });
        }
        let mut sections = [Vec::new(), Vec::new(), Vec::new()];
        for (section, &count) in sections.iter_mut().zip(&counts[1..]) {
            for _ in 0..count {
                let (name, n) = read_name(b, pos)?;
                pos += n;
                let rtype = u16_at(b, pos)?;
                let ttl = u32_at(b, pos + 4)?;
                let len = usize::from(u16_at(b, pos + 8)?);
                pos += 10;
                let rdata = b.get(pos..pos + len).ok_or(NbnsError::Truncated)?.to_vec();
                pos += len;
                section.push(Resource {
                    name,
                    rtype,
                    ttl,
                    rdata,
                });
            }
        }
        let [answers, authority, additional] = sections;
        Ok(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    /// The packet's bytes. A record named as the first question is written
    /// as a pointer to it (offset 12), as RFC 1002's requests are.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        out.extend_from_slice(&self.header.id.to_be_bytes());
        out.extend_from_slice(&self.header.word().to_be_bytes());
        for n in [
            self.questions.len(),
            self.answers.len(),
            self.authority.len(),
            self.additional.len(),
        ] {
            out.extend_from_slice(&u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes());
        }
        for q in &self.questions {
            out.extend_from_slice(&q.name.encode());
            out.extend_from_slice(&q.qtype.to_be_bytes());
            out.extend_from_slice(&CLASS_IN.to_be_bytes());
        }
        let first = self.questions.first().map(|q| &q.name);
        for r in self
            .answers
            .iter()
            .chain(&self.authority)
            .chain(&self.additional)
        {
            if first == Some(&r.name) {
                out.extend_from_slice(&[0xC0, 0x0C]);
            } else {
                out.extend_from_slice(&r.name.encode());
            }
            out.extend_from_slice(&r.rtype.to_be_bytes());
            out.extend_from_slice(&CLASS_IN.to_be_bytes());
            out.extend_from_slice(&r.ttl.to_be_bytes());
            let len = u16::try_from(r.rdata.len()).unwrap_or(u16::MAX);
            out.extend_from_slice(&len.to_be_bytes());
            out.extend_from_slice(&r.rdata[..usize::from(len)]);
        }
        out
    }
}

/// The `RDATA` of a node status response (RFC 1002 4.2.18): the names
/// with their `NAME_FLAGS`, then the 46 bytes of statistics, all zero but
/// for `unit_id`.
#[must_use]
pub fn node_status(names: &[(NbName, u16)], unit_id: [u8; 6]) -> Vec<u8> {
    let names = &names[..names.len().min(255)];
    let mut out = vec![u8::try_from(names.len()).unwrap_or(u8::MAX)];
    for (n, flags) in names {
        let mut raw = [b' '; 16];
        for (d, s) in raw.iter_mut().zip(n.name.bytes().take(15)) {
            *d = s;
        }
        raw[15] = n.suffix;
        out.extend_from_slice(&raw);
        out.extend_from_slice(&flags.to_be_bytes());
    }
    out.extend_from_slice(&unit_id);
    out.extend_from_slice(&[0u8; 40]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbss::SUFFIX_SERVER;

    #[test]
    fn rfc_1001_name_example() {
        // RFC 1001 14.1: "FRED" with the scope NETBIOS.COM.
        let mut n = NbName::new("FRED", SUFFIX_SERVER);
        n.scope = "NETBIOS.COM".into();
        let e = n.encode();
        assert_eq!(e[0], 32);
        assert_eq!(&e[1..33], b"EGFCEFEECACACACACACACACACACACACA");
        assert_eq!(&e[33..], b"\x07NETBIOS\x03COM\x00");
        assert_eq!(NbName::decode(&e).unwrap(), (n, e.len()));
    }

    #[test]
    fn the_wildcard_is_padded_with_zeros() {
        let e = NbName::new("*", 0).encode();
        assert_eq!(&e[1..5], b"CKAA");
        assert_eq!(&e[5..33], [b'A'; 28]);
        assert_eq!(NbName::decode(&e).unwrap().0.name, "*");
    }

    /// RFC 1002 4.2.2: a broadcast registration with its additional record
    /// pointing at the question's name.
    #[test]
    fn a_registration_request_round_trips() {
        let name = NbName::new("QSTEST", 0x20);
        let p = Packet {
            header: Header {
                id: 0x1234,
                opcode: OP_REGISTRATION,
                rd: true,
                broadcast: true,
                ..Header::default()
            },
            questions: vec![Question {
                name: name.clone(),
                qtype: TYPE_NB,
            }],
            additional: vec![Resource::nb(name, 300_000, 0, Ipv4Addr::new(10, 0, 2, 15))],
            ..Packet::default()
        };
        let b = p.encode();
        assert_eq!(&b[..12], [0x12, 0x34, 0x29, 0x10, 0, 1, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&b[12 + 34..12 + 38], [0, 0x20, 0, 1]);
        assert_eq!(&b[50..52], [0xC0, 0x0C]);
        assert_eq!(&b[52..62], [0, 0x20, 0, 1, 0, 4, 0x93, 0xE0, 0, 6]);
        assert_eq!(&b[62..], [0, 0, 10, 0, 2, 15]);
        let back = Packet::parse(&b).unwrap();
        assert_eq!(back, p);
        assert_eq!(
            back.additional[0].addresses(),
            [(0, Ipv4Addr::new(10, 0, 2, 15))]
        );
    }

    #[test]
    fn malformed_packets_are_refused() {
        assert!(matches!(Packet::parse(&[0; 11]), Err(NbnsError::Truncated)));
        let mut b = vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        // A pointer forwards, then to itself.
        b.extend_from_slice(&[0xC0, 0x40]);
        assert!(matches!(Packet::parse(&b), Err(NbnsError::BadPointer)));
        b.truncate(12);
        b.extend_from_slice(&[0xC0, 0x0C]);
        assert!(matches!(Packet::parse(&b), Err(NbnsError::BadPointer)));
        let many = [0, 1, 0, 0, 0, 17, 0, 0, 0, 0, 0, 0];
        assert!(matches!(Packet::parse(&many), Err(NbnsError::TooMany)));
        assert!(matches!(
            Packet::parse(&[0; MAX_PACKET + 1]),
            Err(NbnsError::TooLong)
        ));
    }

    #[test]
    fn node_status_layout() {
        let d = node_status(
            &[(NbName::new("QSTEST", 0x20), NAME_ACTIVE)],
            [1, 2, 3, 4, 5, 6],
        );
        assert_eq!(d.len(), 1 + 18 + 46);
        assert_eq!(&d[1..17], b"QSTEST         \x20");
        assert_eq!(&d[17..19], [0x04, 0x00]);
        assert_eq!(&d[19..25], [1, 2, 3, 4, 5, 6]);
    }
}
