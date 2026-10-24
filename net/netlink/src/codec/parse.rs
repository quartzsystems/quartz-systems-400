// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Parsing messages and attributes out of a received datagram.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{NLA_F_NESTED, NLA_TYPE_MASK, NLMSG_DONE, NLMSG_ERROR, NLMSG_HDRLEN, align};
use crate::{Error, Result};

/// One message, borrowed from the datagram.
#[derive(Clone, Copy, Debug)]
pub struct Msg<'a> {
    /// `nlmsg_type`.
    pub kind: u16,
    /// `nlmsg_flags`.
    pub flags: u16,
    /// `nlmsg_seq`.
    pub seq: u32,
    /// `nlmsg_pid`.
    pub pid: u32,
    /// Everything after the header.
    pub payload: &'a [u8],
}

impl Msg<'_> {
    /// For `NLMSG_ERROR`: 0 for an acknowledgement, else the positive
    /// errno. `None` for other types.
    pub fn error(&self) -> Option<i32> {
        if self.kind != NLMSG_ERROR {
            return None;
        }
        let code = i32::from_ne_bytes(self.payload.get(0..4)?.try_into().ok()?);
        Some(-code)
    }

    /// For `NLMSG_DONE`: the positive errno a failed dump carries, or 0.
    pub fn done_error(&self) -> Option<i32> {
        if self.kind != NLMSG_DONE {
            return None;
        }
        let code = self
            .payload
            .get(0..4)
            .map_or(0, |b| i32::from_ne_bytes(b.try_into().unwrap_or_default()));
        Some(-code)
    }

    /// The attributes after a fixed family header of `hdr` bytes.
    pub fn attrs(&self, hdr: usize) -> Attrs<'_> {
        Attrs::new(self.payload.get(align(hdr)..).unwrap_or(&[]))
    }
}

/// Iterator over the messages of a datagram.
pub struct Messages<'a> {
    buf: &'a [u8],
}

/// The messages in `buf`.
pub fn messages(buf: &[u8]) -> Messages<'_> {
    Messages { buf }
}

impl<'a> Iterator for Messages<'a> {
    type Item = Result<Msg<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.len() < NLMSG_HDRLEN {
            return None;
        }
        let b = self.buf;
        let len = u32::from_ne_bytes(b[0..4].try_into().ok()?) as usize;
        if len < NLMSG_HDRLEN || len > b.len() {
            self.buf = &[];
            return Some(Err(Error::Malformed("message length")));
        }
        let msg = Msg {
            kind: u16::from_ne_bytes([b[4], b[5]]),
            flags: u16::from_ne_bytes([b[6], b[7]]),
            seq: u32::from_ne_bytes(b[8..12].try_into().ok()?),
            pid: u32::from_ne_bytes(b[12..16].try_into().ok()?),
            payload: &b[NLMSG_HDRLEN..len],
        };
        self.buf = b.get(align(len)..).unwrap_or(&[]);
        Some(Ok(msg))
    }
}

/// One attribute.
#[derive(Clone, Copy, Debug)]
pub struct Attr<'a> {
    /// Type with the flag bits masked off.
    pub kind: u16,
    /// `NLA_F_NESTED` was set.
    pub nested: bool,
    /// The payload.
    pub data: &'a [u8],
}

impl<'a> Attr<'a> {
    /// First byte.
    pub fn u8(&self) -> Option<u8> {
        self.data.first().copied()
    }

    /// Host-order `u16`.
    pub fn u16(&self) -> Option<u16> {
        super::ne_u16(self.data, 0)
    }

    /// Host-order `u32`.
    pub fn u32(&self) -> Option<u32> {
        super::ne_u32(self.data, 0)
    }

    /// Host-order `u64`.
    pub fn u64(&self) -> Option<u64> {
        super::ne_u64(self.data, 0)
    }

    /// Big-endian `u16`.
    pub fn be16(&self) -> Option<u16> {
        super::be_u16(self.data, 0)
    }

    /// Big-endian `u32`.
    pub fn be32(&self) -> Option<u32> {
        super::be_u32(self.data, 0)
    }

    /// Big-endian `u64`.
    pub fn be64(&self) -> Option<u64> {
        Some(u64::from_be_bytes(self.data.get(0..8)?.try_into().ok()?))
    }

    /// A string, up to the first NUL.
    pub fn str(&self) -> String {
        let end = self
            .data
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(self.data.len());
        String::from_utf8_lossy(&self.data[..end]).into_owned()
    }

    /// An IPv4 (4 bytes) or IPv6 (16 bytes) address.
    pub fn ip(&self) -> Option<IpAddr> {
        ip_from(self.data)
    }

    /// The nested attributes.
    pub fn attrs(&self) -> Attrs<'a> {
        Attrs::new(self.data)
    }
}

/// An address from 4 or 16 network-order bytes.
pub fn ip_from(b: &[u8]) -> Option<IpAddr> {
    match b.len() {
        4 => Some(IpAddr::V4(Ipv4Addr::from(<[u8; 4]>::try_from(b).ok()?))),
        16 => Some(IpAddr::V6(Ipv6Addr::from(<[u8; 16]>::try_from(b).ok()?))),
        _ => None,
    }
}

/// Iterator over attributes. Stops at the first malformed one.
#[derive(Clone, Debug)]
pub struct Attrs<'a> {
    buf: &'a [u8],
}

impl<'a> Attrs<'a> {
    /// The attributes in `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    /// The first attribute of `kind`.
    pub fn get(&self, kind: u16) -> Option<Attr<'a>> {
        let mut it = self.clone();
        it.find(|a| a.kind == kind)
    }
}

impl<'a> Iterator for Attrs<'a> {
    type Item = Attr<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.buf;
        if b.len() < 4 {
            return None;
        }
        let len = u16::from_ne_bytes([b[0], b[1]]) as usize;
        let raw = u16::from_ne_bytes([b[2], b[3]]);
        if len < 4 || len > b.len() {
            self.buf = &[];
            return None;
        }
        self.buf = b.get(align(len)..).unwrap_or(&[]);
        Some(Attr {
            kind: raw & NLA_TYPE_MASK,
            nested: raw & NLA_F_NESTED != 0,
            data: &b[4..len],
        })
    }
}
