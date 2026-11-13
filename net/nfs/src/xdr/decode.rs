// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The decoder: XDR from a slice, which is its whole budget. No length
//! is trusted: each is checked against its bound and the bytes left
//! before anything is allocated. Structs and unions count their depth.

use super::Xdr;
use super::encode::pad_len;
use super::error::{Error, Result};

/// How deeply items may nest by default.
pub const MAX_DEPTH: u32 = 64;

/// Arrays reserve at most this many items before they are decoded.
const RESERVE: usize = 256;

/// XDR from a slice.
#[derive(Debug, Clone)]
pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    depth: u32,
    max_depth: u32,
}

impl<'a> Decoder<'a> {
    /// A decoder over `buf` with the default depth limit.
    pub fn new(buf: &'a [u8]) -> Self {
        Self::with_depth(buf, MAX_DEPTH)
    }

    /// A decoder over `buf` allowing `max_depth` levels of nesting.
    pub fn with_depth(buf: &'a [u8], max_depth: u32) -> Self {
        Self {
            buf,
            pos: 0,
            depth: 0,
            max_depth,
        }
    }

    /// Bytes not yet decoded.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Bytes decoded so far.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Whether every byte is decoded.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Fails unless every byte is decoded.
    pub fn finish(&self) -> Result<()> {
        match self.remaining() {
            0 => Ok(()),
            n => Err(Error::Trailing(n)),
        }
    }

    /// The bytes not yet decoded, all taken.
    pub fn rest(&mut self) -> &'a [u8] {
        let r = self.buf.get(self.pos..).unwrap_or_default();
        self.pos = self.buf.len();
        r
    }

    /// One level deeper, failing past the limit.
    pub fn enter(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(Error::TooDeep(self.max_depth));
        }
        Ok(())
    }

    /// One level back.
    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// `n` bytes as they are.
    pub fn get_raw(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.buf.len())
            .ok_or(Error::Short {
                need: n as u64,
                left: self.remaining(),
            })?;
        let b = self.buf.get(self.pos..end).unwrap_or_default();
        self.pos = end;
        Ok(b)
    }

    fn word<const N: usize>(&mut self) -> Result<[u8; N]> {
        let b = self.get_raw(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(b);
        Ok(a)
    }

    /// An `unsigned int`.
    pub fn get_u32(&mut self) -> Result<u32> {
        self.word().map(u32::from_be_bytes)
    }

    /// An `int`.
    pub fn get_i32(&mut self) -> Result<i32> {
        self.word().map(i32::from_be_bytes)
    }

    /// An `unsigned hyper`.
    pub fn get_u64(&mut self) -> Result<u64> {
        self.word().map(u64::from_be_bytes)
    }

    /// A `hyper`.
    pub fn get_i64(&mut self) -> Result<i64> {
        self.word().map(i64::from_be_bytes)
    }

    /// A `bool`: 0 or 1.
    pub fn get_bool(&mut self) -> Result<bool> {
        match self.get_u32()? {
            0 => Ok(false),
            1 => Ok(true),
            n => Err(Error::Bool(n)),
        }
    }

    /// A fixed-length opaque of `n` bytes; its padding must be zero.
    pub fn get_fixed(&mut self, n: usize) -> Result<&'a [u8]> {
        let pad = pad_len(n);
        if n.saturating_add(pad) > self.remaining() {
            return Err(Error::Short {
                need: n as u64 + pad as u64,
                left: self.remaining(),
            });
        }
        let b = self.get_raw(n)?;
        if self.get_raw(pad)?.iter().any(|x| *x != 0) {
            return Err(Error::Padding);
        }
        Ok(b)
    }

    /// A length checked against `max`.
    fn get_len(&mut self, max: u32) -> Result<usize> {
        let n = self.get_u32()?;
        if n > max {
            return Err(Error::TooLong {
                len: u64::from(n),
                max,
            });
        }
        usize::try_from(n).map_err(|_| Error::Short {
            need: u64::from(n),
            left: self.remaining(),
        })
    }

    /// A variable-length opaque or string of at most `max` bytes, borrowed.
    pub fn get_opaque_ref(&mut self, max: u32) -> Result<&'a [u8]> {
        let n = self.get_len(max)?;
        self.get_fixed(n)
    }

    /// A variable-length opaque or string of at most `max` bytes.
    pub fn get_opaque(&mut self, max: u32) -> Result<Vec<u8>> {
        self.get_opaque_ref(max).map(<[u8]>::to_vec)
    }

    /// Checks that `n` items of `T` can fit in what is left.
    fn fits<T: Xdr>(&self, n: usize) -> Result<()> {
        let need = n.saturating_mul(T::MIN_SIZE.max(1));
        if need > self.remaining() {
            return Err(Error::Short {
                need: need as u64,
                left: self.remaining(),
            });
        }
        Ok(())
    }

    fn items<T: Xdr>(&mut self, n: usize) -> Result<Vec<T>> {
        self.fits::<T>(n)?;
        let mut v = Vec::with_capacity(n.min(RESERVE));
        for _ in 0..n {
            v.push(T::decode(self)?);
        }
        Ok(v)
    }

    /// A variable-length array of at most `max` items.
    pub fn get_array<T: Xdr>(&mut self, max: u32) -> Result<Vec<T>> {
        let n = self.get_len(max)?;
        self.items(n)
    }

    /// A fixed-length array of `n` items.
    pub fn get_fixed_array<T: Xdr>(&mut self, n: u32) -> Result<Vec<T>> {
        let n = usize::try_from(n).map_err(|_| Error::Short {
            need: u64::from(n),
            left: self.remaining(),
        })?;
        self.items(n)
    }

    /// A linked list (`T *next` chains), decoded without recursion.
    pub fn get_list<T: Xdr>(&mut self) -> Result<Vec<T>> {
        let mut v = Vec::new();
        while self.get_bool()? {
            v.push(T::decode(self)?);
        }
        Ok(v)
    }
}
