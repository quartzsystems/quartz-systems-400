// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fields of the API formats: EBCDIC (CCSID 37) text padded with
//! blanks, big-endian binary.

/// An EBCDIC blank.
pub const BLANK: u8 = 0x40;

/// `s` as a blank-padded EBCDIC field of `n` bytes; characters CP037
/// lacks become blanks, and the text is cut at `n`.
#[must_use]
pub fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = s
        .chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(BLANK))
        .take(n)
        .collect();
    b.resize(n, BLANK);
    b
}

/// Text of an EBCDIC field, trailing blanks (and NULs) dropped.
#[must_use]
pub fn text(b: &[u8]) -> String {
    let end = b
        .iter()
        .rposition(|&x| x != BLANK && x != 0)
        .map_or(0, |i| i + 1);
    ccsid::from_ebcdic(&b[..end])
}

/// Text of `n` bytes at `off`, blanks when short.
#[must_use]
pub fn text_at(b: &[u8], off: usize, n: usize) -> String {
    b.get(off..off + n).map_or_else(String::new, text)
}

/// The raw field at `off`, padded with blanks when short.
#[must_use]
pub fn raw_at(b: &[u8], off: usize, n: usize) -> Vec<u8> {
    let mut v = b.get(off..).unwrap_or(&[]).to_vec();
    v.truncate(n);
    v.resize(n, BLANK);
    v
}

/// The big-endian `i32` at `off`; 0 when short.
#[must_use]
pub fn bin4(b: &[u8], off: usize) -> i32 {
    b.get(off..off + 4)
        .map_or(0, |x| i32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

/// The big-endian `i64` at `off`; 0 when short.
#[must_use]
pub fn bin8(b: &[u8], off: usize) -> i64 {
    b.get(off..off + 8).map_or(0, |x| {
        let mut a = [0u8; 8];
        a.copy_from_slice(x);
        i64::from_be_bytes(a)
    })
}

/// One EBCDIC character at `off` as a `char`; a blank when short.
#[must_use]
pub fn ch(b: &[u8], off: usize) -> char {
    ccsid::char_of(b.get(off).copied().unwrap_or(BLANK))
}

/// A growable record written at fixed offsets.
#[derive(Clone, Debug, Default)]
pub struct Rec {
    /// The bytes so far.
    pub buf: Vec<u8>,
}

impl Rec {
    /// A record of `n` zero bytes.
    #[must_use]
    pub fn zeros(n: usize) -> Self {
        Self { buf: vec![0; n] }
    }

    fn at(&mut self, off: usize, n: usize) -> &mut [u8] {
        if self.buf.len() < off + n {
            self.buf.resize(off + n, 0);
        }
        &mut self.buf[off..off + n]
    }

    /// A blank-padded EBCDIC field.
    pub fn char(&mut self, off: usize, n: usize, s: &str) {
        let f = fixed(s, n);
        self.at(off, n).copy_from_slice(&f);
    }

    /// Raw bytes.
    pub fn bytes(&mut self, off: usize, b: &[u8]) {
        self.at(off, b.len()).copy_from_slice(b);
    }

    /// A big-endian `i32`.
    pub fn bin4(&mut self, off: usize, v: i32) {
        self.at(off, 4).copy_from_slice(&v.to_be_bytes());
    }

    /// A big-endian `i32` from a size or count (saturating).
    pub fn size4(&mut self, off: usize, v: usize) {
        self.bin4(off, i32::try_from(v).unwrap_or(i32::MAX));
    }

    /// A big-endian `i64`.
    pub fn bin8(&mut self, off: usize, v: i64) {
        self.at(off, 8).copy_from_slice(&v.to_be_bytes());
    }

    /// A flag as the character `0` or `1`.
    pub fn flag(&mut self, off: usize, v: bool) {
        self.at(off, 1)[0] = if v { 0xF1 } else { 0xF0 };
    }

    /// `n` blanks.
    pub fn blanks(&mut self, off: usize, n: usize) {
        self.at(off, n).fill(BLANK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_pad_cut_and_trim() {
        assert_eq!(fixed("QIBM", 6), vec![0xD8, 0xC9, 0xC2, 0xD4, 0x40, 0x40]);
        assert_eq!(fixed("ABCDEF", 3).len(), 3);
        assert_eq!(text(&fixed("QIBM", 12)), "QIBM");
        assert_eq!(bin4(&[0, 0, 4, 0xA4], 0), 1188);
        assert_eq!(bin4(&[0, 0], 0), 0);
        let mut r = Rec::zeros(2);
        r.flag(4, true);
        assert_eq!(r.buf, vec![0, 0, 0, 0, 0xF1]);
    }
}
