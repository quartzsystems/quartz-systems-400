// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data stream structure: a six-byte header (length, `0xD0`, the
//! format byte, the request correlation ID) and the objects. The format
//! byte carries the chaining flags and the type. A DSS longer than
//! 32,767 bytes is cut into segments: the first header's length has the
//! high bit on, and each continuation starts with a two-byte length,
//! high bit on while more follow.

use crate::wire::object::{Object, decode_all};
use crate::{Error, be16};

/// The header's length.
pub const DSS_HEAD: usize = 6;

/// The header's fields: offset, size, name.
pub const HEADER_LAYOUT: &[(usize, usize, &str)] = &[
    (0, 2, "length"),
    (2, 1, "magic"),
    (3, 1, "format"),
    (4, 2, "correlation"),
];
/// The magic byte at offset 2.
pub const MAGIC: u8 = 0xD0;
/// The longest segment.
pub const MAX_SEGMENT: usize = 0x7FFF;

/// Format byte: another DSS follows in this chain.
pub const CHAINED: u8 = 0x40;
/// Format byte: continue the chain when this command fails.
pub const CONTINUE_ON_ERROR: u8 = 0x20;
/// Format byte: the next DSS has the same correlation ID.
pub const SAME_CORRELATION: u8 = 0x10;

/// The type in the format byte's low nibble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DssType {
    /// A command (`RQSDSS`).
    Request,
    /// A reply message (`RPYDSS`).
    Reply,
    /// Command or reply data (`OBJDSS`).
    Object,
    /// Encrypted data (`CMNDSS`).
    Encrypted,
    /// A command that wants no reply.
    RequestNoReply,
    /// Anything else, kept as is.
    Other(u8),
}

impl DssType {
    /// From the low nibble.
    pub fn from_nibble(n: u8) -> Self {
        match n {
            1 => Self::Request,
            2 => Self::Reply,
            3 => Self::Object,
            4 => Self::Encrypted,
            5 => Self::RequestNoReply,
            n => Self::Other(n),
        }
    }

    /// The low nibble.
    pub fn nibble(self) -> u8 {
        match self {
            Self::Request => 1,
            Self::Reply => 2,
            Self::Object => 3,
            Self::Encrypted => 4,
            Self::RequestNoReply => 5,
            Self::Other(n) => n & 0x0F,
        }
    }
}

/// What a DSS carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Payload {
    /// Objects.
    Objects(Vec<Object>),
    /// Bytes that are not objects (encrypted data, say).
    Raw(Vec<u8>),
}

/// One DSS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dss {
    /// The format byte's flags (high nibble).
    pub flags: u8,
    /// The type.
    pub ty: DssType,
    /// The request correlation ID.
    pub corr: u16,
    /// The contents.
    pub payload: Payload,
}

impl Dss {
    /// A DSS of one object.
    pub fn new(ty: DssType, corr: u16, obj: Object) -> Self {
        Self {
            flags: 0,
            ty,
            corr,
            payload: Payload::Objects(vec![obj]),
        }
    }

    /// A command.
    pub fn request(corr: u16, obj: Object) -> Self {
        Self::new(DssType::Request, corr, obj)
    }

    /// A reply message.
    pub fn reply(corr: u16, obj: Object) -> Self {
        Self::new(DssType::Reply, corr, obj)
    }

    /// Command or reply data.
    pub fn object(corr: u16, obj: Object) -> Self {
        Self::new(DssType::Object, corr, obj)
    }

    /// Whether another DSS follows in the chain.
    pub fn chained(&self) -> bool {
        self.flags & CHAINED != 0
    }

    /// Whether the next DSS has the same correlation ID.
    pub fn same_corr(&self) -> bool {
        self.flags & SAME_CORRELATION != 0
    }

    /// The objects (empty for raw data).
    pub fn objects(&self) -> &[Object] {
        match &self.payload {
            Payload::Objects(o) => o,
            Payload::Raw(_) => &[],
        }
    }

    /// The first object.
    pub fn first(&self) -> Option<&Object> {
        self.objects().first()
    }

    /// The first object's code point.
    pub fn code(&self) -> Option<u16> {
        self.first().map(|o| o.cp)
    }

    fn payload_bytes(&self) -> Vec<u8> {
        match &self.payload {
            Payload::Objects(objs) => {
                let mut v = Vec::new();
                for o in objs {
                    o.encode_into(&mut v);
                }
                v
            }
            Payload::Raw(r) => r.clone(),
        }
    }

    /// The encoding, segmented when longer than 32,767 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let body = self.payload_bytes();
        let format = (self.flags & 0xF0) | self.ty.nibble();
        let mut out = Vec::with_capacity(body.len() + DSS_HEAD);
        let first = body.len().min(MAX_SEGMENT - DSS_HEAD);
        let more = first < body.len();
        let ll = u16::try_from(DSS_HEAD + first).unwrap_or(0) | if more { 0x8000 } else { 0 };
        out.extend_from_slice(&ll.to_be_bytes());
        out.push(MAGIC);
        out.push(format);
        out.extend_from_slice(&self.corr.to_be_bytes());
        out.extend_from_slice(&body[..first]);
        let mut at = first;
        while at < body.len() {
            let n = (body.len() - at).min(MAX_SEGMENT - 2);
            let more = at + n < body.len();
            let ll = u16::try_from(n + 2).unwrap_or(0) | if more { 0x8000 } else { 0 };
            out.extend_from_slice(&ll.to_be_bytes());
            out.extend_from_slice(&body[at..at + n]);
            at += n;
        }
        out
    }

    /// Read one DSS from the front of `b`; returns it and the bytes used.
    pub fn decode(b: &[u8]) -> Result<(Self, usize), Error> {
        let total = dss_len(b)?.ok_or(Error::Short {
            what: "DSS",
            need: b.len() + 1,
            have: b.len(),
        })?;
        let format = b[3];
        let corr = be16(b, 4);
        let mut body = Vec::with_capacity(total);
        let ll = usize::from(be16(b, 0) & 0x7FFF);
        body.extend_from_slice(&b[DSS_HEAD..ll]);
        let mut at = ll;
        let mut more = be16(b, 0) & 0x8000 != 0;
        while more {
            let l = be16(b, at);
            more = l & 0x8000 != 0;
            let l = usize::from(l & 0x7FFF);
            body.extend_from_slice(&b[at + 2..at + l]);
            at += l;
        }
        let ty = DssType::from_nibble(format & 0x0F);
        let payload = if ty == DssType::Encrypted {
            Payload::Raw(body)
        } else {
            match decode_all(&body) {
                Ok(objs) => Payload::Objects(objs),
                Err(_) => Payload::Raw(body),
            }
        };
        Ok((
            Self {
                flags: format & 0xF0,
                ty,
                corr,
                payload,
            },
            at,
        ))
    }
}

/// How many bytes the DSS at the front of `b` takes, segments included;
/// `None` while it is incomplete. An error when the header is not one.
pub fn dss_len(b: &[u8]) -> Result<Option<usize>, Error> {
    if b.len() < DSS_HEAD {
        return Ok(None);
    }
    if b[2] != MAGIC {
        return Err(Error::Dss(format!("byte 2 is {:02X}", b[2])));
    }
    let first = be16(b, 0);
    let ll = usize::from(first & 0x7FFF);
    if ll < DSS_HEAD {
        return Err(Error::Dss(format!("length {ll}")));
    }
    let mut at = ll;
    let mut more = first & 0x8000 != 0;
    while more {
        if b.len() < at + 2 {
            return Ok(None);
        }
        let l = be16(b, at);
        more = l & 0x8000 != 0;
        let l = usize::from(l & 0x7FFF);
        if l < 2 {
            return Err(Error::Dss(format!("segment length {l}")));
        }
        at += l;
    }
    Ok((b.len() >= at).then_some(at))
}

/// Set the chaining flags of a list of DSSes sent together: each but the
/// last is chained, and marked same-correlation when the next one shares
/// its correlation ID.
pub fn chain(list: &mut [Dss]) {
    let n = list.len();
    for i in 0..n {
        list[i].flags &= !(CHAINED | SAME_CORRELATION);
        if i + 1 < n {
            list[i].flags |= CHAINED;
            if list[i + 1].corr == list[i].corr {
                list[i].flags |= SAME_CORRELATION;
            }
        }
    }
}

/// Encode a chain.
pub fn encode_chain(list: &[Dss]) -> Vec<u8> {
    let mut v = Vec::new();
    for d in list {
        v.extend(d.encode());
    }
    v
}

/// Decode every DSS in `b`, which must end on a boundary.
pub fn decode_chain(mut b: &[u8]) -> Result<Vec<Dss>, Error> {
    let mut v = Vec::new();
    while !b.is_empty() {
        let (d, n) = Dss::decode(b)?;
        v.push(d);
        b = &b[n..];
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cp;

    #[test]
    fn header_and_flags() {
        let mut l = vec![
            Dss::request(1, Object::coll(cp::cmd::PRPSQLSTT, vec![])),
            Dss::object(1, Object::data(cp::obj::SQLSTT, vec![1, 2])),
            Dss::request(2, Object::coll(cp::cmd::OPNQRY, vec![])),
        ];
        chain(&mut l);
        let b = encode_chain(&l);
        assert_eq!(&b[..6], &[0, 10, 0xD0, 0x51, 0, 1]);
        assert_eq!(decode_chain(&b).unwrap(), l);
    }

    #[test]
    fn segments() {
        let d = Dss::object(3, Object::data(cp::obj::QRYDTA, vec![9u8; 70000]));
        let b = d.encode();
        assert_eq!(be16(&b, 0), 0xFFFF);
        assert_eq!(dss_len(&b).unwrap(), Some(b.len()));
        assert_eq!(dss_len(&b[..b.len() - 1]).unwrap(), None);
        let (back, n) = Dss::decode(&b).unwrap();
        assert_eq!(n, b.len());
        assert_eq!(back.first().unwrap().bytes().len(), 70000);
        assert_eq!(back.encode(), b);
    }
}
