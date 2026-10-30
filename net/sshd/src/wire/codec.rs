// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RFC 4251 data types: `byte`, `boolean`, `uint32`, `uint64`,
//! `string`, `mpint` and `name-list`.

use thiserror::Error;

/// A message ended early or held a value out of range.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("malformed message: {0}")]
pub struct WireError(pub &'static str);

/// A result of decoding.
pub type WireResult<T> = Result<T, WireError>;

/// Builds a message.
#[derive(Debug, Default, Clone)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    /// An empty message.
    #[must_use]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// A message starting with its type byte.
    #[must_use]
    pub fn msg(kind: u8) -> Self {
        Self(vec![kind])
    }

    /// `byte`.
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    /// `boolean`.
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.0.push(u8::from(v));
        self
    }

    /// `uint32`.
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// `uint64`.
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// `string` of bytes.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(len32(v.len()));
        self.0.extend_from_slice(v);
        self
    }

    /// `string` of text.
    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }

    /// `mpint` of an unsigned big-endian magnitude.
    pub fn mpint(&mut self, v: &[u8]) -> &mut Self {
        let v = strip_zeros(v);
        if v.first().is_some_and(|b| b & 0x80 != 0) {
            self.u32(len32(v.len() + 1));
            self.0.push(0);
        } else {
            self.u32(len32(v.len()));
        }
        self.0.extend_from_slice(v);
        self
    }

    /// `name-list`.
    pub fn names(&mut self, v: &[&str]) -> &mut Self {
        self.str(&v.join(","))
    }

    /// Raw bytes, no length.
    pub fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }

    /// The message.
    #[must_use]
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

fn len32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn strip_zeros(v: &[u8]) -> &[u8] {
    let n = v.iter().take_while(|b| **b == 0).count();
    &v[n..]
}

/// Reads a message.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Over `buf`.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    /// What is left.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.buf
    }

    /// Whether everything was read.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// `n` raw bytes.
    pub fn take(&mut self, n: usize) -> WireResult<&'a [u8]> {
        if self.buf.len() < n {
            return Err(WireError("short"));
        }
        let (a, b) = self.buf.split_at(n);
        self.buf = b;
        Ok(a)
    }

    /// `byte`.
    pub fn u8(&mut self) -> WireResult<u8> {
        Ok(self.take(1)?[0])
    }

    /// `boolean`.
    pub fn bool(&mut self) -> WireResult<bool> {
        Ok(self.u8()? != 0)
    }

    /// `uint32`.
    pub fn u32(&mut self) -> WireResult<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// `uint64`.
    pub fn u64(&mut self) -> WireResult<u64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    /// `string` of bytes.
    pub fn bytes(&mut self) -> WireResult<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    /// `string` of UTF-8 text.
    pub fn str(&mut self) -> WireResult<&'a str> {
        std::str::from_utf8(self.bytes()?).map_err(|_| WireError("text is not UTF-8"))
    }

    /// `mpint`, as its unsigned magnitude (negative values refused).
    pub fn mpint(&mut self) -> WireResult<&'a [u8]> {
        let v = self.bytes()?;
        if v.first().is_some_and(|b| b & 0x80 != 0) {
            return Err(WireError("negative mpint"));
        }
        Ok(strip_zeros(v))
    }

    /// `name-list`.
    pub fn names(&mut self) -> WireResult<Vec<&'a str>> {
        let s = self.str()?;
        Ok(if s.is_empty() {
            Vec::new()
        } else {
            s.split(',').collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4251_mpint_examples() {
        let mut w = Writer::new();
        w.mpint(&[]);
        assert_eq!(w.finish(), [0, 0, 0, 0]);
        w.mpint(&[0x09, 0xa3, 0x78, 0xf9, 0xb2, 0xe3, 0x32, 0xa7]);
        assert_eq!(w.finish(), [0, 0, 0, 8, 0x09, 0xa3, 0x78, 0xf9, 0xb2, 0xe3, 0x32, 0xa7]);
        w.mpint(&[0x80]);
        assert_eq!(w.finish(), [0, 0, 0, 2, 0, 0x80]);
        w.mpint(&[0, 0, 0x7f]);
        assert_eq!(w.finish(), [0, 0, 0, 1, 0x7f]);
    }

    #[test]
    fn round_trip() {
        let mut w = Writer::msg(20);
        w.u32(7).str("abc").names(&["a", "b"]).bool(true).u64(9);
        let b = w.finish();
        let mut r = Reader::new(&b);
        assert_eq!(r.u8().unwrap(), 20);
        assert_eq!(r.u32().unwrap(), 7);
        assert_eq!(r.str().unwrap(), "abc");
        assert_eq!(r.names().unwrap(), ["a", "b"]);
        assert!(r.bool().unwrap());
        assert_eq!(r.u64().unwrap(), 9);
        assert!(r.is_empty());
        assert!(r.u8().is_err());
    }
}
