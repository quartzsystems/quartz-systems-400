// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The encoder: XDR into a `Vec<u8>`, padding with zeros to 4 bytes.

use super::error::{Error, Result};
use super::Xdr;

/// XDR into a growing buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Encoder {
    buf: Vec<u8>,
}

/// Zero bytes to pad `n` to a multiple of four.
pub(crate) fn pad_len(n: usize) -> usize {
    (4 - n % 4) % 4
}

impl Encoder {
    /// An empty encoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// An encoder that continues `buf`.
    pub fn from_vec(buf: Vec<u8>) -> Self {
        Self { buf }
    }

    /// The bytes so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// The bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }

    /// How many bytes so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether nothing is written yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// An `unsigned int`.
    pub fn put_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// An `int`.
    pub fn put_i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// An `unsigned hyper`.
    pub fn put_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A `hyper`.
    pub fn put_i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A `bool`.
    pub fn put_bool(&mut self, v: bool) {
        self.put_u32(u32::from(v));
    }

    /// Bytes as they are: no length, no padding (a body already encoded).
    pub fn put_raw(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// A fixed-length opaque: the bytes and their padding.
    pub fn put_fixed(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
        self.buf.resize(self.buf.len() + pad_len(b.len()), 0);
    }

    /// A length checked against `max`.
    fn put_len(&mut self, len: usize, max: u32) -> Result<()> {
        let n = u32::try_from(len).ok().filter(|n| *n <= max).ok_or(Error::TooLong {
            len: len as u64,
            max,
        })?;
        self.put_u32(n);
        Ok(())
    }

    /// A variable-length opaque or string of at most `max` bytes.
    pub fn put_opaque(&mut self, b: &[u8], max: u32) -> Result<()> {
        self.put_len(b.len(), max)?;
        self.put_fixed(b);
        Ok(())
    }

    /// A variable-length array of at most `max` items.
    pub fn put_array<T: Xdr>(&mut self, v: &[T], max: u32) -> Result<()> {
        self.put_len(v.len(), max)?;
        v.iter().try_for_each(|x| x.encode(self))
    }

    /// A fixed-length array of exactly `n` items.
    pub fn put_fixed_array<T: Xdr>(&mut self, v: &[T], n: u32) -> Result<()> {
        if u32::try_from(v.len()).ok() != Some(n) {
            return Err(Error::FixedLength { len: v.len(), want: n });
        }
        v.iter().try_for_each(|x| x.encode(self))
    }

    /// A linked list (`T *next` chains): each item after TRUE, then FALSE.
    pub fn put_list<T: Xdr>(&mut self, v: &[T]) -> Result<()> {
        for x in v {
            self.put_bool(true);
            x.encode(self)?;
        }
        self.put_bool(false);
        Ok(())
    }
}
