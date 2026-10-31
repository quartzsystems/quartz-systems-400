// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSZ* and QLZA* APIs over parameter bytes, so the integrator can
//! install them as natives the way `sysapi` does: read each parameter's
//! bytes at its address, call the function here, write the receiver it
//! returns, and report an [`ApiError`] through the error code
//! parameter. Character fields are EBCDIC (CCSID 37) and blank padded;
//! binary fields are big-endian.

pub mod chktg;
pub mod crtpd;
pub mod crtpl;
pub mod dispatch;
pub mod lic;
pub mod pkgpo;
pub mod rtvpr;
pub mod sptpr;

use thiserror::Error;

/// An API failure: the message identifier and its replacement data.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("{id}")]
pub struct ApiError {
    /// `CPF0C1F`, ...
    pub id: &'static str,
    /// Replacement data, EBCDIC.
    pub data: Vec<u8>,
}

impl ApiError {
    /// A failure with no data.
    #[must_use]
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            data: Vec::new(),
        }
    }

    /// Append a text field of `n` bytes to the data.
    #[must_use]
    pub fn text(mut self, s: &str, n: usize) -> Self {
        self.data.extend(fixed(s, n));
        self
    }
}

/// A blank-padded EBCDIC field of `n` bytes (characters with no CP037
/// form become blanks).
#[must_use]
pub fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = s
        .chars()
        .take(n)
        .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
        .collect();
    b.resize(n, ccsid::SPACE);
    b
}

/// A character field of an input, trimmed; empty when short.
#[must_use]
pub fn char_at(b: &[u8], off: usize, n: usize) -> String {
    b.get(off..off + n)
        .map(ccsid::decode_trimmed)
        .unwrap_or_default()
}

/// A big-endian `i32` of an input; 0 when short.
#[must_use]
pub fn bin4_at(b: &[u8], off: usize) -> i32 {
    b.get(off..off + 4)
        .map_or(0, |x| i32::from_be_bytes(x.try_into().expect("4 bytes")))
}

/// Whether a field of an input is all blanks (or absent).
#[must_use]
pub fn blank_at(b: &[u8], off: usize, n: usize) -> bool {
    b.get(off..off + n)
        .is_none_or(|x| x.iter().all(|c| *c == ccsid::SPACE))
}

/// A receiver being filled at fixed offsets.
#[derive(Default)]
pub struct Rcv(pub Vec<u8>);

impl Rcv {
    fn grow(&mut self, end: usize) {
        if self.0.len() < end {
            self.0.resize(end, 0);
        }
    }

    /// A character field.
    pub fn char(&mut self, off: usize, n: usize, s: &str) {
        self.grow(off + n);
        self.0[off..off + n].copy_from_slice(&fixed(s, n));
    }

    /// A big-endian `i32`.
    pub fn bin4(&mut self, off: usize, v: i32) {
        self.grow(off + 4);
        self.0[off..off + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Raw bytes.
    pub fn bytes(&mut self, off: usize, b: &[u8]) {
        self.grow(off + b.len());
        self.0[off..off + b.len()].copy_from_slice(b);
    }

    /// Blanks (reserved character fields).
    pub fn blank(&mut self, off: usize, n: usize) {
        self.char(off, n, "");
    }

    /// The current length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether nothing was written.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Pad with zeros to a multiple of four.
    pub fn align4(&mut self) {
        let n = self.0.len().next_multiple_of(4);
        self.grow(n);
    }

    /// Set bytes returned and available and cut to the receiver's
    /// length. A length under 8 is `CPF3C24`.
    pub fn deliver(mut self, len: i32) -> Result<Vec<u8>, ApiError> {
        let len = usize::try_from(len).map_err(|_| ApiError::new("CPF3C24"))?;
        if len < 8 {
            return Err(ApiError::new("CPF3C24"));
        }
        let avail = self.0.len();
        let ret = avail.min(len);
        self.bin4(0, i32::try_from(ret).unwrap_or(i32::MAX));
        self.bin4(4, i32::try_from(avail).unwrap_or(i32::MAX));
        self.0.truncate(ret);
        Ok(self.0)
    }
}

/// `*YES`/`*NO` to a flag.
pub(crate) fn yes_no(s: &str, err: &'static str) -> Result<bool, ApiError> {
    match s {
        "*YES" => Ok(true),
        "*NO" => Ok(false),
        _ => Err(ApiError::new(err)),
    }
}
