// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CP037 (EBCDIC, US and Canada) to and from UTF-8, and EBCDIC
//! collation. The table comes from `cp037.csv` in this crate, written
//! from the published code page layout.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod cp037;

use std::cmp::Ordering;

pub use cp037::TO_UNICODE;

/// EBCDIC space.
pub const SPACE: u8 = 0x40;

/// A character CP037 cannot represent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("character {ch:?} (U+{code:04X}) has no CP037 encoding")]
pub struct Unmappable {
    /// The character.
    pub ch: char,
    /// Its code point.
    pub code: u32,
}

/// Unicode to CP037 for the 256 mapped code points, built once.
fn reverse() -> &'static [(u32, u8)] {
    use std::sync::OnceLock;
    static REV: OnceLock<Vec<(u32, u8)>> = OnceLock::new();
    REV.get_or_init(|| {
        let mut v: Vec<(u32, u8)> = TO_UNICODE
            .iter()
            .enumerate()
            .map(|(b, &cp)| (cp, u8::try_from(b).expect("byte")))
            .collect();
        v.sort_unstable();
        v
    })
}

/// The CP037 byte for `ch`, if any.
#[must_use]
pub fn byte_of(ch: char) -> Option<u8> {
    let code = u32::from(ch);
    let rev = reverse();
    rev.binary_search_by_key(&code, |&(cp, _)| cp)
        .ok()
        .map(|i| rev[i].1)
}

/// The character for a CP037 byte. Every byte maps.
#[must_use]
pub fn char_of(byte: u8) -> char {
    char::from_u32(TO_UNICODE[usize::from(byte)]).expect("table holds scalar values")
}

/// Whether CP037 can represent `ch`.
#[must_use]
pub fn is_representable(ch: char) -> bool {
    byte_of(ch).is_some()
}

/// UTF-8 to CP037. Fails on the first character without an encoding.
pub fn to_ebcdic(s: &str) -> Result<Vec<u8>, Unmappable> {
    s.chars()
        .map(|ch| {
            byte_of(ch).ok_or(Unmappable {
                ch,
                code: u32::from(ch),
            })
        })
        .collect()
}

/// CP037 to UTF-8.
#[must_use]
pub fn from_ebcdic(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char_of(b)).collect()
}

/// Encode into a fixed field, padded with EBCDIC spaces. Fails when a
/// character has no encoding or the text is longer than the field.
pub fn encode_fixed(s: &str, out: &mut [u8]) -> Result<(), FieldError> {
    let bytes = to_ebcdic(s)?;
    if bytes.len() > out.len() {
        return Err(FieldError::TooLong {
            len: bytes.len(),
            field: out.len(),
        });
    }
    out[..bytes.len()].copy_from_slice(&bytes);
    out[bytes.len()..].fill(SPACE);
    Ok(())
}

/// Decode a fixed field, dropping trailing spaces.
#[must_use]
pub fn decode_trimmed(field: &[u8]) -> String {
    let end = field.iter().rposition(|&b| b != SPACE).map_or(0, |i| i + 1);
    from_ebcdic(&field[..end])
}

/// Why a fixed field could not be filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FieldError {
    /// A character has no encoding.
    #[error(transparent)]
    Unmappable(#[from] Unmappable),
    /// The text does not fit.
    #[error("{len} bytes do not fit a {field}-byte field")]
    TooLong {
        /// Encoded length.
        len: usize,
        /// Field length.
        field: usize,
    },
}

/// Collate two CP037 strings the way the system does: byte by byte,
/// shorter padded with spaces. `$`, `#` and `@` sort before letters,
/// letters before digits.
#[must_use]
pub fn ebcdic_cmp(left: &[u8], right: &[u8]) -> Ordering {
    let len = left.len().max(right.len());
    for at in 0..len {
        let lb = left.get(at).copied().unwrap_or(SPACE);
        let rb = right.get(at).copied().unwrap_or(SPACE);
        match lb.cmp(&rb) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

/// [`ebcdic_cmp`] on UTF-8 text. Characters without an encoding sort
/// last, by code point.
#[must_use]
pub fn ebcdic_cmp_str(left: &str, right: &str) -> Ordering {
    let key = |s: &str| -> Vec<(u8, u32)> {
        s.chars()
            .map(|c| byte_of(c).map_or((0xFF, u32::from(c) + 1), |b| (b, 0)))
            .collect()
    };
    let (lk, rk) = (key(left), key(right));
    let len = lk.len().max(rk.len());
    for at in 0..len {
        let lb = lk.get(at).copied().unwrap_or((SPACE, 0));
        let rb = rk.get(at).copied().unwrap_or((SPACE, 0));
        match lb.cmp(&rb) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_points() {
        assert_eq!(byte_of(' '), Some(0x40));
        assert_eq!(byte_of('A'), Some(0xC1));
        assert_eq!(byte_of('a'), Some(0x81));
        assert_eq!(byte_of('0'), Some(0xF0));
        assert_eq!(byte_of('$'), Some(0x5B));
        assert_eq!(byte_of('#'), Some(0x7B));
        assert_eq!(byte_of('@'), Some(0x7C));
        assert_eq!(byte_of('_'), Some(0x6D));
        assert_eq!(byte_of('"'), Some(0x7F));
        assert_eq!(byte_of('\n'), Some(0x25));
        assert_eq!(byte_of('¬'), Some(0x5F));
        assert_eq!(byte_of('€'), None);
        assert_eq!(char_of(0x15), '\u{85}');
        assert_eq!(char_of(0xFF), '\u{9f}');
    }

    #[test]
    fn every_byte_round_trips() {
        for b in 0..=255u8 {
            assert_eq!(byte_of(char_of(b)), Some(b), "byte {b:#04x}");
        }
        assert_eq!(
            from_ebcdic(&to_ebcdic("Hello, QS/400!").unwrap()),
            "Hello, QS/400!"
        );
    }

    #[test]
    fn fixed_fields() {
        let mut f = [0u8; 10];
        encode_fixed("QSYS", &mut f).unwrap();
        assert_eq!(
            f,
            [0xD8, 0xE2, 0xE8, 0xE2, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40]
        );
        assert_eq!(decode_trimmed(&f), "QSYS");
        assert_eq!(decode_trimmed(&[0x40; 4]), "");
        assert!(matches!(
            encode_fixed("ABCDEFGHIJK", &mut f),
            Err(FieldError::TooLong { .. })
        ));
        assert!(matches!(
            encode_fixed("€", &mut f),
            Err(FieldError::Unmappable(_))
        ));
    }

    #[test]
    fn collation_is_ebcdic() {
        let mut names = vec!["QSYS", "A1", "$X", "#X", "@X", "1ABC", "AB", "A", "Z"];
        names.sort_by(|a, b| ebcdic_cmp_str(a, b));
        assert_eq!(
            names,
            vec!["$X", "#X", "@X", "A", "AB", "A1", "QSYS", "Z", "1ABC"]
        );
        assert_eq!(ebcdic_cmp(b"\xC1", b"\xC1\x40\x40"), Ordering::Equal);
        assert_eq!(ebcdic_cmp(b"\xC1", b"\xC1\xC1"), Ordering::Less);
    }
}
