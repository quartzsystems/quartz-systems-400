// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FD:OCA data: the SQLCA group of `SQLCARD`, the SQLDA of `SQLDARD`,
//! the answer set descriptor of `QRYDSC` and the rows of `QRYDTA`.
//! Integers follow the `TYPDEFNAM` of `ACCRDB` (big-endian for
//! `QTDSQL400`); characters are in the `CCSIDSBC`.

pub mod desc;
pub mod rows;
pub mod sqlca;
pub mod sqlda;
pub mod types;

use crate::Error;

/// How data is represented on a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Repr {
    /// Integers little-endian.
    pub little: bool,
    /// The single-byte CCSID.
    pub ccsid: u16,
}

impl Default for Repr {
    fn default() -> Self {
        Self {
            little: false,
            ccsid: 37,
        }
    }
}

/// Builds FD:OCA bytes.
#[derive(Debug, Default)]
pub struct Writer {
    /// The bytes so far.
    pub out: Vec<u8>,
    /// The representation.
    pub repr: Repr,
}

impl Writer {
    /// A writer for `repr`.
    pub fn new(repr: Repr) -> Self {
        Self {
            out: Vec::new(),
            repr,
        }
    }

    /// One byte.
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    /// A two-byte integer.
    pub fn i16(&mut self, v: i16) {
        if self.repr.little {
            self.out.extend(v.to_le_bytes());
        } else {
            self.out.extend(v.to_be_bytes());
        }
    }

    /// A four-byte integer.
    pub fn i32(&mut self, v: i32) {
        if self.repr.little {
            self.out.extend(v.to_le_bytes());
        } else {
            self.out.extend(v.to_be_bytes());
        }
    }

    /// An eight-byte integer.
    pub fn i64(&mut self, v: i64) {
        if self.repr.little {
            self.out.extend(v.to_le_bytes());
        } else {
            self.out.extend(v.to_be_bytes());
        }
    }

    /// A two-byte big-endian value (FD:OCA lengths, CCSIDs).
    pub fn be16(&mut self, v: u16) {
        self.out.extend(v.to_be_bytes());
    }

    /// Characters padded with blanks to `len`.
    pub fn fixed(&mut self, text: &str, len: usize) {
        let mut b = self.chars(text);
        b.resize(len, self.blank());
        self.out.extend(b);
    }

    /// A string with a two-byte length (`VCS`, `VCM`).
    pub fn vcs(&mut self, text: &str) {
        let b = self.chars(text);
        self.be16(u16::try_from(b.len()).unwrap_or(u16::MAX));
        self.out.extend(b);
    }

    /// Bytes as they are.
    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    /// Text in the representation's CCSID.
    pub fn chars(&self, text: &str) -> Vec<u8> {
        ccsid::conv::from_str(self.repr.ccsid, text).unwrap_or_else(|_| crate::wire::ebcdic(text))
    }

    fn blank(&self) -> u8 {
        if self.repr.ccsid == 37 || ccsid::conv::codepage_of(self.repr.ccsid) == 37 {
            0x40
        } else {
            self.chars(" ").first().copied().unwrap_or(0x40)
        }
    }
}

/// Reads FD:OCA bytes.
#[derive(Debug)]
pub struct Reader<'a> {
    b: &'a [u8],
    /// Where the next read starts.
    pub at: usize,
    /// The representation.
    pub repr: Repr,
}

impl<'a> Reader<'a> {
    /// A reader over `b`.
    pub fn new(b: &'a [u8], repr: Repr) -> Self {
        Self { b, at: 0, repr }
    }

    /// Bytes left.
    pub fn rest(&self) -> &'a [u8] {
        &self.b[self.at.min(self.b.len())..]
    }

    /// Whether everything was read.
    pub fn done(&self) -> bool {
        self.at >= self.b.len()
    }

    /// The next `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.at + n > self.b.len() {
            return Err(Error::Short {
                what: "FD:OCA data",
                need: self.at + n,
                have: self.b.len(),
            });
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    /// One byte.
    pub fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    /// The next byte without reading it.
    pub fn peek(&self) -> Option<u8> {
        self.b.get(self.at).copied()
    }

    /// A two-byte integer.
    pub fn i16(&mut self) -> Result<i16, Error> {
        let b: [u8; 2] = self.take(2)?.try_into().expect("2");
        Ok(if self.repr.little {
            i16::from_le_bytes(b)
        } else {
            i16::from_be_bytes(b)
        })
    }

    /// A four-byte integer.
    pub fn i32(&mut self) -> Result<i32, Error> {
        let b: [u8; 4] = self.take(4)?.try_into().expect("4");
        Ok(if self.repr.little {
            i32::from_le_bytes(b)
        } else {
            i32::from_be_bytes(b)
        })
    }

    /// An eight-byte integer.
    pub fn i64(&mut self) -> Result<i64, Error> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("8");
        Ok(if self.repr.little {
            i64::from_le_bytes(b)
        } else {
            i64::from_be_bytes(b)
        })
    }

    /// A two-byte big-endian value.
    pub fn be16(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// Fixed characters, trailing blanks removed.
    pub fn fixed(&mut self, len: usize) -> Result<String, Error> {
        let b = self.take(len)?;
        Ok(self.text(b).trim_end().to_string())
    }

    /// A string with a two-byte length.
    pub fn vcs(&mut self) -> Result<String, Error> {
        let n = usize::from(self.be16()?);
        let b = self.take(n)?;
        Ok(self.text(b))
    }

    /// Bytes as text in the representation's CCSID.
    pub fn text(&self, b: &[u8]) -> String {
        ccsid::conv::to_string(self.repr.ccsid, b)
            .unwrap_or_else(|_| b.iter().map(|&x| ccsid::char_of(x)).collect())
    }
}
