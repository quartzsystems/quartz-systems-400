// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DNS messages (RFC 1035): the header, questions and resource records,
//! names with compression on decode. Encoding never compresses; the
//! resolver's queries carry one name and the test server's answers are
//! small.

use std::net::{Ipv4Addr, Ipv6Addr};

use thiserror::Error;

use crate::consts::{C_IN, T_A, T_AAAA, T_CNAME, T_PTR};

/// A malformed message.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// It ends inside a field.
    #[error("the message is truncated")]
    Truncated,
    /// A compression pointer loops or points forward.
    #[error("a name's compression pointers loop")]
    PointerLoop,
    /// A label over 63 bytes or a name over 255.
    #[error("a name is too long")]
    NameTooLong,
}

type R<T> = Result<T, CodecError>;

/// Response codes.
pub mod rcode {
    /// No error.
    pub const NOERROR: u8 = 0;
    /// The server could not parse the query.
    pub const FORMERR: u8 = 1;
    /// The server failed.
    pub const SERVFAIL: u8 = 2;
    /// The name does not exist.
    pub const NXDOMAIN: u8 = 3;
    /// Not implemented.
    pub const NOTIMP: u8 = 4;
    /// Refused.
    pub const REFUSED: u8 = 5;
}

/// The 12-byte header; the flag bits are flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Header {
    /// Query identifier.
    pub id: u16,
    /// A response.
    pub qr: bool,
    /// Operation, 0 for a standard query.
    pub opcode: u8,
    /// Authoritative answer.
    pub aa: bool,
    /// Truncated (retry over TCP).
    pub tc: bool,
    /// Recursion desired.
    pub rd: bool,
    /// Recursion available.
    pub ra: bool,
    /// Response code.
    pub rcode: u8,
}

/// A question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The name, without the final dot.
    pub name: String,
    /// `T_*`.
    pub qtype: u16,
    /// `C_IN`.
    pub qclass: u16,
}

/// Record data the resolver understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rdata {
    /// `A`.
    A(Ipv4Addr),
    /// `AAAA`.
    Aaaa(Ipv6Addr),
    /// `CNAME`.
    Cname(String),
    /// `PTR`.
    Ptr(String),
    /// Anything else, raw.
    Other(Vec<u8>),
}

/// A resource record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Owner name.
    pub name: String,
    /// `T_*`.
    pub rtype: u16,
    /// Class.
    pub class: u16,
    /// Time to live.
    pub ttl: u32,
    /// The data.
    pub data: Rdata,
}

impl Record {
    /// An `IN` record.
    #[must_use]
    pub fn new(name: &str, ttl: u32, data: Rdata) -> Self {
        let rtype = match &data {
            Rdata::A(_) => T_A,
            Rdata::Aaaa(_) => T_AAAA,
            Rdata::Cname(_) => T_CNAME,
            Rdata::Ptr(_) => T_PTR,
            Rdata::Other(_) => 0,
        };
        Self {
            name: name.to_string(),
            rtype,
            class: C_IN,
            ttl,
            data,
        }
    }
}

/// A message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message {
    /// Header.
    pub header: Header,
    /// Questions.
    pub questions: Vec<Question>,
    /// Answers.
    pub answers: Vec<Record>,
    /// Authority records.
    pub authority: Vec<Record>,
    /// Additional records.
    pub additional: Vec<Record>,
}

impl Message {
    /// A standard query for one name.
    #[must_use]
    pub fn query(id: u16, name: &str, qtype: u16, qclass: u16, recurse: bool) -> Self {
        Self {
            header: Header {
                id,
                rd: recurse,
                ..Header::default()
            },
            questions: vec![Question {
                name: name.trim_end_matches('.').to_string(),
                qtype,
                qclass,
            }],
            ..Self::default()
        }
    }

    /// The wire form.
    pub fn encode(&self) -> R<Vec<u8>> {
        let h = &self.header;
        let mut b = Vec::with_capacity(512);
        b.extend_from_slice(&h.id.to_be_bytes());
        let mut f1 = (h.opcode & 0x0f) << 3;
        if h.qr {
            f1 |= 0x80;
        }
        if h.aa {
            f1 |= 0x04;
        }
        if h.tc {
            f1 |= 0x02;
        }
        if h.rd {
            f1 |= 0x01;
        }
        let mut f2 = h.rcode & 0x0f;
        if h.ra {
            f2 |= 0x80;
        }
        b.push(f1);
        b.push(f2);
        for n in [
            self.questions.len(),
            self.answers.len(),
            self.authority.len(),
            self.additional.len(),
        ] {
            b.extend_from_slice(&u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes());
        }
        for q in &self.questions {
            put_name(&mut b, &q.name)?;
            b.extend_from_slice(&q.qtype.to_be_bytes());
            b.extend_from_slice(&q.qclass.to_be_bytes());
        }
        for r in self.answers.iter().chain(&self.authority).chain(&self.additional) {
            put_name(&mut b, &r.name)?;
            b.extend_from_slice(&r.rtype.to_be_bytes());
            b.extend_from_slice(&r.class.to_be_bytes());
            b.extend_from_slice(&r.ttl.to_be_bytes());
            let mut d = Vec::new();
            match &r.data {
                Rdata::A(a) => d.extend_from_slice(&a.octets()),
                Rdata::Aaaa(a) => d.extend_from_slice(&a.octets()),
                Rdata::Cname(n) | Rdata::Ptr(n) => put_name(&mut d, n)?,
                Rdata::Other(o) => d.extend_from_slice(o),
            }
            b.extend_from_slice(&u16::try_from(d.len()).unwrap_or(u16::MAX).to_be_bytes());
            b.extend_from_slice(&d);
        }
        Ok(b)
    }

    /// From the wire form.
    pub fn decode(b: &[u8]) -> R<Self> {
        if b.len() < 12 {
            return Err(CodecError::Truncated);
        }
        let u16_at = |i: usize| u16::from_be_bytes([b[i], b[i + 1]]);
        let header = Header {
            id: u16_at(0),
            qr: b[2] & 0x80 != 0,
            opcode: (b[2] >> 3) & 0x0f,
            aa: b[2] & 0x04 != 0,
            tc: b[2] & 0x02 != 0,
            rd: b[2] & 0x01 != 0,
            ra: b[3] & 0x80 != 0,
            rcode: b[3] & 0x0f,
        };
        let (qd, an, ns, ar) = (u16_at(4), u16_at(6), u16_at(8), u16_at(10));
        let mut at = 12;
        let mut m = Message {
            header,
            ..Self::default()
        };
        for _ in 0..qd {
            let name = get_name(b, &mut at)?;
            let f = b.get(at..at + 4).ok_or(CodecError::Truncated)?;
            m.questions.push(Question {
                name,
                qtype: u16::from_be_bytes([f[0], f[1]]),
                qclass: u16::from_be_bytes([f[2], f[3]]),
            });
            at += 4;
        }
        for (n, list) in [(an, 0), (ns, 1), (ar, 2)] {
            for _ in 0..n {
                let r = get_record(b, &mut at)?;
                match list {
                    0 => m.answers.push(r),
                    1 => m.authority.push(r),
                    _ => m.additional.push(r),
                }
            }
        }
        Ok(m)
    }
}

fn put_name(b: &mut Vec<u8>, name: &str) -> R<()> {
    let name = name.trim_end_matches('.');
    let mut total = 1;
    if !name.is_empty() {
        for label in name.split('.') {
            let l = label.as_bytes();
            if l.is_empty() || l.len() > 63 {
                return Err(CodecError::NameTooLong);
            }
            total += l.len() + 1;
            b.push(u8::try_from(l.len()).expect("63 at most"));
            b.extend_from_slice(l);
        }
    }
    if total > 255 {
        return Err(CodecError::NameTooLong);
    }
    b.push(0);
    Ok(())
}

/// A name at `*at`, following compression pointers; `*at` moves past
/// the name's bytes in place.
fn get_name(b: &[u8], at: &mut usize) -> R<String> {
    let mut labels: Vec<String> = Vec::new();
    let mut pos = *at;
    let mut jumped = false;
    let mut hops = 0;
    let mut len = 0;
    loop {
        let n = *b.get(pos).ok_or(CodecError::Truncated)?;
        match n & 0xc0 {
            0xc0 => {
                let lo = *b.get(pos + 1).ok_or(CodecError::Truncated)?;
                let target = usize::from(n & 0x3f) << 8 | usize::from(lo);
                if !jumped {
                    *at = pos + 2;
                }
                jumped = true;
                hops += 1;
                if hops > 64 || target >= pos {
                    return Err(CodecError::PointerLoop);
                }
                pos = target;
            }
            0 => {
                if n == 0 {
                    if !jumped {
                        *at = pos + 1;
                    }
                    break;
                }
                let l = usize::from(n);
                let s = b.get(pos + 1..pos + 1 + l).ok_or(CodecError::Truncated)?;
                len += l + 1;
                if len > 255 {
                    return Err(CodecError::NameTooLong);
                }
                labels.push(String::from_utf8_lossy(s).into_owned());
                pos += 1 + l;
            }
            _ => return Err(CodecError::Truncated),
        }
    }
    Ok(labels.join("."))
}

fn get_record(b: &[u8], at: &mut usize) -> R<Record> {
    let name = get_name(b, at)?;
    let f = b.get(*at..*at + 10).ok_or(CodecError::Truncated)?;
    let rtype = u16::from_be_bytes([f[0], f[1]]);
    let class = u16::from_be_bytes([f[2], f[3]]);
    let ttl = u32::from_be_bytes([f[4], f[5], f[6], f[7]]);
    let rdlen = usize::from(u16::from_be_bytes([f[8], f[9]]));
    let start = *at + 10;
    let raw = b.get(start..start + rdlen).ok_or(CodecError::Truncated)?;
    let data = match rtype {
        T_A if rdlen == 4 => Rdata::A(Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3])),
        T_AAAA if rdlen == 16 => Rdata::Aaaa(Ipv6Addr::from(<[u8; 16]>::try_from(raw).expect("16"))),
        T_CNAME | T_PTR => {
            let mut p = start;
            let n = get_name(b, &mut p)?;
            if rtype == T_CNAME {
                Rdata::Cname(n)
            } else {
                Rdata::Ptr(n)
            }
        }
        _ => Rdata::Other(raw.to_vec()),
    };
    *at = start + rdlen;
    Ok(Record {
        name,
        rtype,
        class,
        ttl,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_encodes_as_rfc1035_says() {
        let q = Message::query(0x1234, "www.example.com.", T_A, C_IN, true);
        let b = q.encode().unwrap();
        assert_eq!(&b[..12], &[0x12, 0x34, 0x01, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&b[12..], b"\x03www\x07example\x03com\x00\x00\x01\x00\x01");
        assert_eq!(Message::decode(&b).unwrap(), q);
    }

    #[test]
    fn compressed_answers_decode() {
        // A response for www.example.com: a CNAME to web.example.com and
        // its A record, both names compressed against the question.
        let mut b = vec![0xab, 0xcd, 0x81, 0x80, 0, 1, 0, 2, 0, 0, 0, 0];
        b.extend_from_slice(b"\x03www\x07example\x03com\x00\x00\x01\x00\x01");
        b.extend_from_slice(&[0xc0, 12, 0, 5, 0, 1, 0, 0, 0, 60, 0, 6]);
        b.extend_from_slice(&[3, b'w', b'e', b'b', 0xc0, 16]);
        let web = 12 + 17 + 4 + 12;
        b.extend_from_slice(&[0xc0, u8::try_from(web).unwrap(), 0, 1, 0, 1, 0, 0, 1, 0, 0, 4, 192, 0, 2, 7]);
        let m = Message::decode(&b).unwrap();
        assert!(m.header.qr && m.header.rd && m.header.ra);
        assert_eq!(m.header.rcode, rcode::NOERROR);
        assert_eq!(m.questions[0].name, "www.example.com");
        assert_eq!(m.answers[0].data, Rdata::Cname("web.example.com".into()));
        assert_eq!(m.answers[1].name, "web.example.com");
        assert_eq!(m.answers[1].data, Rdata::A(Ipv4Addr::new(192, 0, 2, 7)));
        assert_eq!(m.answers[1].ttl, 256);
    }

    #[test]
    fn bad_messages_are_errors() {
        assert_eq!(Message::decode(&[0; 5]), Err(CodecError::Truncated));
        let mut b = vec![0, 0, 0x80, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        b.extend_from_slice(&[0xc0, 12]);
        assert_eq!(Message::decode(&b), Err(CodecError::PointerLoop));
        let long = "a".repeat(64);
        assert_eq!(Message::query(1, &long, T_A, C_IN, true).encode(), Err(CodecError::NameTooLong));
        let mut q = Message::query(1, "x", T_AAAA, C_IN, false).encode().unwrap();
        q.truncate(q.len() - 1);
        assert_eq!(Message::decode(&q), Err(CodecError::Truncated));
    }

    #[test]
    fn records_round_trip() {
        let mut m = Message::query(9, "1.2.0.192.in-addr.arpa", T_PTR, C_IN, true);
        m.header.qr = true;
        m.header.aa = true;
        m.header.tc = true;
        m.header.rcode = rcode::NXDOMAIN;
        m.answers.push(Record::new("1.2.0.192.in-addr.arpa", 5, Rdata::Ptr("h.example".into())));
        m.answers.push(Record::new("h.example", 5, Rdata::Aaaa(Ipv6Addr::LOCALHOST)));
        m.additional.push(Record {
            rtype: 16,
            ..Record::new("h.example", 5, Rdata::Other(b"\x02hi".to_vec()))
        });
        let back = Message::decode(&m.encode().unwrap()).unwrap();
        assert_eq!(back, m);
    }
}
