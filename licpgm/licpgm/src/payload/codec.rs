// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Big-endian fields, EBCDIC fixed character fields and UTF-8 runs.

use crate::payload::PayloadError;

/// Appends fields to a buffer.
#[derive(Default)]
pub struct Writer {
    /// What was written.
    pub buf: Vec<u8>,
}

impl Writer {
    /// A `u8`.
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    /// A big-endian `u16`.
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A big-endian `u32`.
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// A big-endian `u64`.
    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    /// Raw bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// `n` zero bytes.
    pub fn zeros(&mut self, n: usize) {
        self.buf.resize(self.buf.len() + n, 0);
    }

    /// A count as `u16`.
    pub fn count16(&mut self, n: usize, field: &'static str) -> Result<(), PayloadError> {
        self.u16(u16::try_from(n).map_err(|_| PayloadError::TooMany(field))?);
        Ok(())
    }

    /// An EBCDIC field of `n` bytes, blank padded.
    pub fn char(&mut self, s: &str, n: usize, field: &'static str) -> Result<(), PayloadError> {
        let mut b = vec![0u8; n];
        ccsid::encode_fixed(s, &mut b).map_err(|_| PayloadError::Text(field))?;
        self.buf.extend_from_slice(&b);
        Ok(())
    }

    /// A UTF-8 run of known length (the length is written elsewhere).
    pub fn utf8(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }
}

/// Reads fields from a buffer.
pub struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// A reader at the start.
    #[must_use]
    pub fn new(b: &'a [u8]) -> Self {
        Self { b, at: 0 }
    }

    /// The offset reached.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.at
    }

    /// `n` raw bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], PayloadError> {
        let s = self
            .b
            .get(self.at..self.at + n)
            .ok_or(PayloadError::Short(self.at + n))?;
        self.at += n;
        Ok(s)
    }

    /// A `u8`.
    pub fn u8(&mut self) -> Result<u8, PayloadError> {
        Ok(self.take(1)?[0])
    }

    /// A big-endian `u16`.
    pub fn u16(&mut self) -> Result<u16, PayloadError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// A big-endian `u32`.
    pub fn u32(&mut self) -> Result<u32, PayloadError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes(b.try_into().expect("4 bytes")))
    }

    /// A big-endian `u64`.
    pub fn u64(&mut self) -> Result<u64, PayloadError> {
        let b = self.take(8)?;
        Ok(u64::from_be_bytes(b.try_into().expect("8 bytes")))
    }

    /// An EBCDIC field, trailing blanks dropped.
    pub fn char(&mut self, n: usize) -> Result<String, PayloadError> {
        Ok(ccsid::decode_trimmed(self.take(n)?))
    }

    /// A UTF-8 run.
    pub fn utf8(&mut self, n: usize, field: &'static str) -> Result<String, PayloadError> {
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| PayloadError::Text(field))
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<(), PayloadError> {
        self.take(n).map(|_| ())
    }
}

/// Write a length-prefixed UTF-8 text entry (`PrdLodText`).
pub fn put_text(w: &mut Writer, s: &str, field: &'static str) -> Result<(), PayloadError> {
    w.count16(s.len(), field)?;
    w.utf8(s);
    Ok(())
}

/// Read a `PrdLodText` entry.
pub fn get_text(r: &mut Reader<'_>, field: &'static str) -> Result<String, PayloadError> {
    let n = usize::from(r.u16()?);
    r.utf8(n, field)
}
