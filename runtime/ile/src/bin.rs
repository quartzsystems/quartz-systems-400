// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The byte codec of the payloads: little-endian integers, `u32`
//! length-prefixed strings and byte arrays, `u32`-counted lists,
//! 10-byte CP037 names.

use crate::error::{IleError, Result};

/// A payload writer.
#[derive(Default)]
pub(crate) struct W {
    pub out: Vec<u8>,
}

impl W {
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.u32(len32(b.len()));
        self.out.extend_from_slice(b);
    }
    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    /// A 10-byte CP037 name, blank padded.
    pub fn name(&mut self, s: &str) {
        let mut b = [ccsid::SPACE; 10];
        let _ = ccsid::encode_fixed(s, &mut b);
        self.out.extend_from_slice(&b);
    }
    /// A fixed CP037 text field.
    pub fn text<const N: usize>(&mut self, s: &str) {
        let mut b = [ccsid::SPACE; N];
        let _ = ccsid::encode_fixed(s, &mut b);
        self.out.extend_from_slice(&b);
    }
    pub fn raw(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }
    pub fn zeros(&mut self, n: usize) {
        self.out.resize(self.out.len() + n, 0);
    }
    pub fn list<T>(&mut self, v: &[T], mut f: impl FnMut(&mut Self, &T)) {
        self.u32(len32(v.len()));
        for x in v {
            f(self, x);
        }
    }
    /// Overwrite a `u64` written earlier.
    pub fn patch_u64(&mut self, at: usize, v: u64) {
        self.out[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    /// Overwrite a `u32` written earlier.
    pub fn patch_u32(&mut self, at: usize, v: u32) {
        self.out[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
}

pub(crate) fn len32(n: usize) -> u32 {
    u32::try_from(n).expect("fewer than 2^32 items")
}

/// A payload reader.
pub(crate) struct R<'a> {
    b: &'a [u8],
    pub at: usize,
    what: &'static str,
}

impl<'a> R<'a> {
    pub fn new(b: &'a [u8], what: &'static str) -> Self {
        Self { b, at: 0, what }
    }
    pub fn err(&self, msg: impl Into<String>) -> IleError {
        IleError::payload(self.what, msg)
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| self.err("length overflow"))?;
        if end > self.b.len() {
            return Err(self.err(format!("truncated at byte {}", self.at)));
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    pub fn seek(&mut self, at: usize) -> Result<()> {
        if at > self.b.len() {
            return Err(self.err(format!("offset {at} past the end")));
        }
        self.at = at;
        Ok(())
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn u64(&mut self) -> Result<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
    pub fn i64(&mut self) -> Result<i64> {
        let b = self.take(8)?;
        Ok(i64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
    pub fn bool(&mut self) -> Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            v => Err(self.err(format!("{v} is not a boolean"))),
        }
    }
    pub fn bytes(&mut self) -> Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    pub fn str(&mut self) -> Result<String> {
        String::from_utf8(self.bytes()?).map_err(|_| self.err("string is not UTF-8"))
    }
    pub fn name(&mut self) -> Result<String> {
        let b = self.take(10)?;
        Ok(ccsid::decode_trimmed(b))
    }
    pub fn text(&mut self, n: usize) -> Result<String> {
        let b = self.take(n)?;
        Ok(ccsid::decode_trimmed(b))
    }
    pub fn list<T>(&mut self, mut f: impl FnMut(&mut Self) -> Result<T>) -> Result<Vec<T>> {
        let n = self.u32()? as usize;
        if n > self.b.len() - self.at {
            return Err(self.err(format!("list of {n} items at byte {}", self.at)));
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(f(self)?);
        }
        Ok(out)
    }
}
