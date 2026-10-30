// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text on the wire: EBCDIC (CCSID 37, the job's), UTF-16 big endian
//! (CCSIDs 1200 and 13488), fixed blank-padded names.

/// `s` in EBCDIC; characters 37 lacks become `?`.
pub fn ebcdic(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0x6F))
        .collect()
}

/// `s` in EBCDIC, blank padded or cut to `n` bytes.
pub fn ebcdic_fixed(s: &str, n: usize) -> Vec<u8> {
    let mut v = ebcdic(s);
    v.resize(n, 0x40);
    v
}

/// A ten-byte EBCDIC name.
pub fn name10(s: &str) -> [u8; 10] {
    let mut a = [0x40u8; 10];
    let v = ebcdic(s);
    let n = v.len().min(10);
    a[..n].copy_from_slice(&v[..n]);
    a
}

/// EBCDIC bytes as text, trailing blanks and nulls removed.
pub fn from_ebcdic(b: &[u8]) -> String {
    ccsid::from_ebcdic(b)
        .trim_end_matches([' ', '\0'])
        .to_string()
}

/// `s` in UTF-16 big endian.
pub fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// UTF-16 big endian bytes as text.
pub fn from_utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .chunks(2)
        .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Bytes in `ccsid` as text: UTF-16 for 1200, 13488 and 61952, UTF-8
/// for 1208, the code page tables otherwise (37 when unknown).
pub fn decode(ccsid: u32, b: &[u8]) -> String {
    match ccsid {
        1200 | 13488 | 61952 => from_utf16(b),
        1208 => String::from_utf8_lossy(b).into_owned(),
        0 | 37 | 65535 => ccsid::from_ebcdic(b),
        c => u16::try_from(c)
            .ok()
            .and_then(|c| ccsid::conv::to_string(c, b).ok())
            .unwrap_or_else(|| ccsid::from_ebcdic(b)),
    }
}

/// Text as bytes in `ccsid` (the inverse of [`decode`]).
pub fn encode(ccsid: u32, s: &str) -> Vec<u8> {
    match ccsid {
        1200 | 13488 | 61952 => utf16(s),
        1208 => s.as_bytes().to_vec(),
        0 | 37 | 65535 => ebcdic(s),
        c => u16::try_from(c)
            .ok()
            .and_then(|c| ccsid::conv::from_str(c, s).ok())
            .unwrap_or_else(|| ebcdic(s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(from_ebcdic(&name10("QSYS")), "QSYS");
        assert_eq!(from_utf16(&utf16("/home/a")), "/home/a");
        assert_eq!(decode(37, &encode(37, "Abc")), "Abc");
    }
}
