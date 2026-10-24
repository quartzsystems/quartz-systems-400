// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Conversion between CCSIDs: the single-byte EBCDIC and ASCII tables,
//! UTF-8 (1208), UTF-16 big-endian (1200, 13488) and binary (65535, no
//! conversion). A character the target cannot hold becomes the target's
//! substitution character, as IBM's conversions do; line ends are just
//! characters and convert like any other.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::tables::TABLES;

/// Binary data: never converted.
pub const BINARY: u16 = 65535;
/// UTF-8.
pub const UTF8: u16 = 1208;
/// UTF-16, big-endian.
pub const UTF16: u16 = 1200;
/// UCS-2, treated as UTF-16 big-endian.
pub const UCS2: u16 = 13488;

/// A CCSID this crate cannot convert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("CCSID {0} is not supported for conversion")]
pub struct Unsupported(pub u16);

/// How a CCSID encodes characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// One byte per character through a table.
    Sbcs {
        /// Byte to code point; `u32::MAX` is unmapped.
        table: &'static [u32; 256],
        /// EBCDIC (substitution 0x3F) or ASCII-based (0x1A).
        ebcdic: bool,
    },
    /// UTF-8.
    Utf8,
    /// UTF-16 big-endian.
    Utf16,
}

/// A convertible CCSID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codec {
    /// The CCSID.
    pub ccsid: u16,
    /// Its encoding.
    pub form: Form,
}

/// The EBCDIC CCSIDs with tables.
const EBCDIC: [u16; 10] = [37, 273, 277, 278, 280, 284, 285, 297, 500, 871];

/// Every CCSID [`Codec::of`] knows, binary included.
#[must_use]
pub fn supported() -> Vec<u16> {
    let mut v = vec![37];
    v.extend(TABLES.iter().map(|(c, _)| *c));
    v.extend([UTF8, UTF16, UCS2, BINARY]);
    v.sort_unstable();
    v
}

/// The CCSID a code page names (`O_CODEPAGE`); the code pages here are
/// their own CCSIDs.
#[must_use]
pub fn ccsid_of_codepage(cp: u16) -> u16 {
    cp
}

/// The code page of a CCSID, for `st_codepage`.
#[must_use]
pub fn codepage_of(ccsid: u16) -> u16 {
    match ccsid {
        UCS2 => UTF16,
        c => c,
    }
}

impl Codec {
    /// The codec for `ccsid`; `None` for binary (65535).
    pub fn of(ccsid: u16) -> Result<Option<Self>, Unsupported> {
        let form = match ccsid {
            BINARY => return Ok(None),
            UTF8 => Form::Utf8,
            UTF16 | UCS2 => Form::Utf16,
            37 => Form::Sbcs {
                table: &crate::TO_UNICODE,
                ebcdic: true,
            },
            c => {
                let (_, table) = TABLES.iter().find(|(n, _)| *n == c).ok_or(Unsupported(c))?;
                Form::Sbcs {
                    table,
                    ebcdic: EBCDIC.contains(&c),
                }
            }
        };
        Ok(Some(Self { ccsid, form }))
    }

    /// Whether this is an EBCDIC CCSID.
    #[must_use]
    pub fn is_ebcdic(&self) -> bool {
        matches!(self.form, Form::Sbcs { ebcdic: true, .. })
    }

    /// Decode as much of `bytes` as forms whole characters into `out`;
    /// returns the bytes used (an incomplete trailing sequence is left).
    pub fn decode(&self, bytes: &[u8], out: &mut Vec<char>) -> usize {
        match self.form {
            Form::Sbcs { table, .. } => {
                out.extend(
                    bytes
                        .iter()
                        .map(|&b| char::from_u32(table[usize::from(b)]).unwrap_or('\u{1A}')),
                );
                bytes.len()
            }
            Form::Utf8 => decode_utf8(bytes, out),
            Form::Utf16 => decode_utf16(bytes, out),
        }
    }

    /// Encode one character, substituting when it has no encoding.
    /// Returns whether it was encoded exactly.
    pub fn encode(&self, ch: char, out: &mut Vec<u8>) -> bool {
        match self.form {
            Form::Sbcs { ebcdic, .. } => {
                if let Some(&b) = reverse(self.ccsid).get(&u32::from(ch)) {
                    out.push(b);
                    true
                } else {
                    out.push(if ebcdic { 0x3F } else { 0x1A });
                    false
                }
            }
            Form::Utf8 => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                true
            }
            Form::Utf16 => {
                let mut buf = [0u16; 2];
                for u in ch.encode_utf16(&mut buf) {
                    out.extend_from_slice(&u.to_be_bytes());
                }
                true
            }
        }
    }
}

/// The reverse map of a CCSID's table, built once per CCSID. Keyed by
/// CCSID: a table's address is not stable when it is a constant.
fn reverse(ccsid: u16) -> &'static HashMap<u32, u8> {
    static REV: OnceLock<HashMap<u16, HashMap<u32, u8>>> = OnceLock::new();
    let all = REV.get_or_init(|| {
        let of = |t: &[u32; 256]| -> HashMap<u32, u8> {
            t.iter()
                .enumerate()
                .filter(|(_, cp)| **cp != u32::MAX)
                .map(|(b, &cp)| (cp, u8::try_from(b).expect("byte")))
                .collect()
        };
        let mut m = HashMap::new();
        m.insert(37, of(&crate::TO_UNICODE));
        for (c, t) in TABLES {
            m.insert(*c, of(t));
        }
        m
    });
    &all[&ccsid]
}

fn decode_utf8(bytes: &[u8], out: &mut Vec<char>) -> usize {
    let mut at = 0;
    while at < bytes.len() {
        match std::str::from_utf8(&bytes[at..]) {
            Ok(s) => {
                out.extend(s.chars());
                return bytes.len();
            }
            Err(e) => {
                let good = e.valid_up_to();
                out.extend(
                    std::str::from_utf8(&bytes[at..at + good])
                        .expect("valid prefix")
                        .chars(),
                );
                at += good;
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        at += n;
                    }
                    None => return at,
                }
            }
        }
    }
    at
}

fn decode_utf16(bytes: &[u8], out: &mut Vec<char>) -> usize {
    let mut at = 0;
    while at + 2 <= bytes.len() {
        let u = u16::from_be_bytes([bytes[at], bytes[at + 1]]);
        if (0xD800..0xDC00).contains(&u) {
            if at + 4 > bytes.len() {
                return at;
            }
            let l = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]);
            if (0xDC00..0xE000).contains(&l) {
                let cp = 0x10000 + ((u32::from(u) - 0xD800) << 10) + (u32::from(l) - 0xDC00);
                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                at += 4;
                continue;
            }
            out.push('\u{FFFD}');
            at += 2;
            continue;
        }
        out.push(char::from_u32(u32::from(u)).unwrap_or('\u{FFFD}'));
        at += 2;
    }
    at
}

/// A streaming conversion from one CCSID to another that carries a
/// partial character between calls.
#[derive(Debug, Clone)]
pub struct Converter {
    from: Option<Codec>,
    to: Option<Codec>,
    pending: Vec<u8>,
    /// Characters that had to be substituted so far.
    pub substituted: usize,
}

impl Converter {
    /// A converter from `from` to `to`.
    pub fn new(from: u16, to: u16) -> Result<Self, Unsupported> {
        Ok(Self {
            from: Codec::of(from)?,
            to: Codec::of(to)?,
            pending: Vec::new(),
            substituted: 0,
        })
    }

    /// Whether bytes pass through unchanged (same CCSID, or either is
    /// binary).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match (self.from, self.to) {
            (Some(a), Some(b)) => a.ccsid == b.ccsid || a.form == b.form,
            _ => true,
        }
    }

    /// Convert `input` and append to `out`. A trailing partial character
    /// waits for the next call.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) {
        if self.is_identity() {
            out.extend_from_slice(input);
            return;
        }
        let (Some(from), Some(to)) = (self.from, self.to) else {
            return;
        };
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(input);
        let mut chars = Vec::with_capacity(buf.len());
        let used = from.decode(&buf, &mut chars);
        for ch in chars {
            if !to.encode(ch, out) {
                self.substituted += 1;
            }
        }
        self.pending = buf[used..].to_vec();
    }

    /// Flush a trailing partial character as one substitution.
    pub fn finish(&mut self, out: &mut Vec<u8>) {
        if self.pending.is_empty() {
            return;
        }
        self.pending.clear();
        if let Some(to) = self.to {
            to.encode('\u{FFFD}', out);
            self.substituted += 1;
        }
    }

    /// Bytes held back waiting for the rest of a character.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

/// Convert a whole buffer.
pub fn convert(from: u16, to: u16, bytes: &[u8]) -> Result<Vec<u8>, Unsupported> {
    let mut c = Converter::new(from, to)?;
    let mut out = Vec::with_capacity(bytes.len());
    c.feed(bytes, &mut out);
    c.finish(&mut out);
    Ok(out)
}

/// Decode a whole buffer to text.
pub fn to_string(ccsid: u16, bytes: &[u8]) -> Result<String, Unsupported> {
    let out = convert(ccsid, UTF8, bytes)?;
    Ok(String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into()))
}

/// Encode text in `ccsid`.
pub fn from_str(ccsid: u16, s: &str) -> Result<Vec<u8>, Unsupported> {
    convert(UTF8, ccsid, s.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ccsid_is_known() {
        for c in [
            37, 273, 277, 278, 280, 284, 285, 297, 500, 871, 819, 850, 1252, 367, 1208, 1200,
            13488, 65535,
        ] {
            assert!(Codec::of(c).is_ok(), "{c}");
            assert!(supported().contains(&c), "{c}");
        }
        assert_eq!(Codec::of(1234), Err(Unsupported(1234)));
    }

    #[test]
    fn sbcs_tables_round_trip() {
        for c in supported() {
            let Ok(Some(codec)) = Codec::of(c) else {
                continue;
            };
            let Form::Sbcs { table, .. } = codec.form else {
                continue;
            };
            for b in 0..=255u8 {
                if table[usize::from(b)] == u32::MAX {
                    continue;
                }
                let mut chars = Vec::new();
                codec.decode(&[b], &mut chars);
                let mut back = Vec::new();
                assert!(codec.encode(chars[0], &mut back), "{c} {b:#04x}");
                assert_eq!(back, vec![b], "{c} {b:#04x}");
            }
        }
    }

    #[test]
    fn national_variants_differ() {
        // '@' is 0x7C in 37 but 0xB5 in 273 (German).
        assert_eq!(from_str(37, "@").unwrap(), vec![0x7C]);
        assert_eq!(from_str(273, "@").unwrap(), vec![0xB5]);
        assert_eq!(from_str(285, "£").unwrap(), vec![0x5B]);
        assert_eq!(from_str(819, "é").unwrap(), vec![0xE9]);
        assert_eq!(from_str(850, "é").unwrap(), vec![0x82]);
        assert_eq!(from_str(1252, "€").unwrap(), vec![0x80]);
        assert_eq!(from_str(367, "é").unwrap(), vec![0x1A]);
        assert_eq!(from_str(37, "€").unwrap(), vec![0x3F]);
    }

    #[test]
    fn line_ends_convert_as_characters() {
        assert_eq!(
            convert(819, 37, b"a\r\nb").unwrap(),
            vec![0x81, 0x0D, 0x25, 0x82]
        );
        assert_eq!(convert(37, 1208, &[0x15]).unwrap(), "\u{85}".as_bytes());
    }

    #[test]
    fn unicode_forms() {
        let s = "Grüße 😀";
        let u16 = from_str(1200, s).unwrap();
        assert_eq!(&u16[..2], &[0, b'G']);
        assert_eq!(to_string(13488, &u16).unwrap(), s);
        assert_eq!(convert(1200, 1208, &u16).unwrap(), s.as_bytes());
        assert_eq!(convert(65535, 37, b"raw").unwrap(), b"raw");
    }

    #[test]
    fn streaming_carries_partial_characters() {
        let s = "aé😀b".as_bytes();
        let mut c = Converter::new(1208, 1200).unwrap();
        let mut out = Vec::new();
        for b in s {
            c.feed(std::slice::from_ref(b), &mut out);
        }
        c.finish(&mut out);
        assert_eq!(out, from_str(1200, "aé😀b").unwrap());
        let mut c = Converter::new(1208, 37).unwrap();
        let mut out = Vec::new();
        c.feed(&[0xC3], &mut out);
        assert_eq!(c.pending(), 1);
        c.finish(&mut out);
        assert_eq!(out, vec![0x3F]);
        assert_eq!(c.substituted, 1);
    }
}
