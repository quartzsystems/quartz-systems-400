// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Building one netlink message.

use super::{NLA_F_NESTED, NLMSG_HDRLEN, align};

/// One message under construction. The length, sequence number and port
/// are filled by [`Builder::finish`].
#[derive(Clone, Debug)]
pub struct Builder {
    buf: Vec<u8>,
}

impl Builder {
    /// A message of `kind` with `flags`.
    pub fn new(kind: u16, flags: u16) -> Self {
        let mut buf = vec![0u8; NLMSG_HDRLEN];
        buf[4..6].copy_from_slice(&kind.to_ne_bytes());
        buf[6..8].copy_from_slice(&flags.to_ne_bytes());
        Self { buf }
    }

    /// The message type.
    pub fn kind(&self) -> u16 {
        u16::from_ne_bytes([self.buf[4], self.buf[5]])
    }

    /// The flags.
    pub fn flags(&self) -> u16 {
        u16::from_ne_bytes([self.buf[6], self.buf[7]])
    }

    /// Add flags.
    pub fn add_flags(&mut self, flags: u16) -> &mut Self {
        let f = self.flags() | flags;
        self.buf[6..8].copy_from_slice(&f.to_ne_bytes());
        self
    }

    /// Raw bytes (a family header), padded to alignment.
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self.pad();
        self
    }

    /// One attribute.
    pub fn attr(&mut self, kind: u16, data: &[u8]) -> &mut Self {
        let len = u16::try_from(4 + data.len()).expect("attribute under 64 KiB");
        self.buf.extend_from_slice(&len.to_ne_bytes());
        self.buf.extend_from_slice(&kind.to_ne_bytes());
        self.buf.extend_from_slice(data);
        self.pad();
        self
    }

    /// A `u8` attribute.
    pub fn u8(&mut self, kind: u16, v: u8) -> &mut Self {
        self.attr(kind, &[v])
    }

    /// A host-order `u16` attribute.
    pub fn u16(&mut self, kind: u16, v: u16) -> &mut Self {
        self.attr(kind, &v.to_ne_bytes())
    }

    /// A host-order `u32` attribute.
    pub fn u32(&mut self, kind: u16, v: u32) -> &mut Self {
        self.attr(kind, &v.to_ne_bytes())
    }

    /// A big-endian `u16` attribute.
    pub fn be16(&mut self, kind: u16, v: u16) -> &mut Self {
        self.attr(kind, &v.to_be_bytes())
    }

    /// A big-endian `u32` attribute.
    pub fn be32(&mut self, kind: u16, v: u32) -> &mut Self {
        self.attr(kind, &v.to_be_bytes())
    }

    /// A big-endian `u64` attribute.
    pub fn be64(&mut self, kind: u16, v: u64) -> &mut Self {
        self.attr(kind, &v.to_be_bytes())
    }

    /// A NUL-terminated string attribute.
    pub fn str(&mut self, kind: u16, s: &str) -> &mut Self {
        let mut v = Vec::with_capacity(s.len() + 1);
        v.extend_from_slice(s.as_bytes());
        v.push(0);
        self.attr(kind, &v)
    }

    /// A nested attribute whose contents `f` adds.
    pub fn nest(&mut self, kind: u16, f: impl FnOnce(&mut Self)) -> &mut Self {
        let start = self.buf.len();
        self.buf.extend_from_slice(&[0, 0]);
        self.buf.extend_from_slice(&(kind | NLA_F_NESTED).to_ne_bytes());
        f(self);
        let len = u16::try_from(self.buf.len() - start).expect("nest under 64 KiB");
        self.buf[start..start + 2].copy_from_slice(&len.to_ne_bytes());
        self
    }

    /// The finished message with sequence number `seq` and port 0.
    pub fn finish(&self, seq: u32) -> Vec<u8> {
        let mut out = self.buf.clone();
        let len = u32::try_from(out.len()).expect("message under 4 GiB");
        out[0..4].copy_from_slice(&len.to_ne_bytes());
        out[8..12].copy_from_slice(&seq.to_ne_bytes());
        out
    }

    fn pad(&mut self) {
        self.buf.resize(align(self.buf.len()), 0);
    }
}
