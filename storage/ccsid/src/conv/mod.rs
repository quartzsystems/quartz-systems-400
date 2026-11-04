// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Conversion between CCSIDs: the single-byte EBCDIC and ASCII tables,
//! the Japanese mixed EBCDIC CCSIDs (930, 5026, 939, 5035) and the
//! double-byte 300, UTF-8 (1208), UTF-16 big-endian (1200, 13488) and
//! binary (65535, no conversion). A character the target cannot hold
//! becomes the target's substitution character, as IBM's conversions do;
//! line ends are just characters and convert like any other. Text cut
//! to a record or field length goes through [`fit`], [`cut`] or
//! [`wrap`], which keep a mixed record's double-byte runs whole.

pub(crate) mod dbcs;
mod fit;
mod mixed;
mod stream;
mod unicode;

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::tables::{SB0930, SB0939, TABLES};

pub use dbcs::DbcsTable;
pub use fit::{Fitted, cut, fit, wrap};
pub use mixed::{DBCS_SPACE, DBCS_SUB, SI, SO};
pub use stream::{Converter, Shift};

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
    /// Mixed EBCDIC: single bytes, and pairs between SO (0x0E) and SI
    /// (0x0F).
    Mixed {
        /// The single-byte half; `u32::MAX` is unmapped.
        sbcs: &'static [u32; 256],
        /// The double-byte half.
        dbcs: &'static DbcsTable,
    },
    /// Pure double-byte EBCDIC: pairs only, no shift codes.
    Dbcs {
        /// The pairs.
        dbcs: &'static DbcsTable,
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

/// The single-byte EBCDIC CCSIDs with tables.
const EBCDIC: [u16; 12] = [37, 273, 277, 278, 280, 284, 285, 297, 500, 871, 1140, 1141];

/// The Japanese CCSIDs: mixed 930, 939, 5026, 5035 and double-byte 300.
const JAPANESE: [u16; 5] = [300, 930, 939, 5026, 5035];

/// Every CCSID [`Codec::of`] knows, binary included.
#[must_use]
pub fn supported() -> Vec<u16> {
    let mut v = vec![37];
    v.extend(TABLES.iter().map(|(c, _)| *c));
    v.extend(JAPANESE);
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
            930 | 5026 => Form::Mixed {
                sbcs: &SB0930,
                dbcs: &dbcs::JA930,
            },
            939 | 5035 => Form::Mixed {
                sbcs: &SB0939,
                dbcs: &dbcs::JA939,
            },
            300 => Form::Dbcs { dbcs: &dbcs::JA300 },
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

    /// Whether this is an EBCDIC CCSID: single-byte, mixed or
    /// double-byte. Its blank is 0x40 (0x4040 in double-byte data) and
    /// its controls (NL 0x15, LF 0x25, CR 0x0D) never occur inside a
    /// double-byte pair, but only the single-byte ones hold one
    /// character per byte.
    #[must_use]
    pub fn is_ebcdic(&self) -> bool {
        matches!(
            self.form,
            Form::Sbcs { ebcdic: true, .. } | Form::Mixed { .. } | Form::Dbcs { .. }
        )
    }

    /// Whether this is a mixed single- and double-byte CCSID, whose
    /// bytes mean different things after SO and after SI.
    #[must_use]
    pub fn is_mixed(&self) -> bool {
        matches!(self.form, Form::Mixed { .. })
    }

    /// Decode as much of `bytes` as forms whole characters into `out`;
    /// returns the bytes used (an incomplete trailing sequence is left).
    /// Mixed data starts in single-byte state; [`Converter`] carries the
    /// state between calls.
    pub fn decode(&self, bytes: &[u8], out: &mut Vec<char>) -> usize {
        let mut shift = self.initial_shift();
        self.decode_in(bytes, &mut shift, out)
    }

    /// Encode one character, substituting when it has no encoding.
    /// Returns whether it was encoded exactly. A double-byte character
    /// of a mixed CCSID comes with its own SO and SI.
    pub fn encode(&self, ch: char, out: &mut Vec<u8>) -> bool {
        let mut shift = Shift::Single;
        let ok = self.encode_in(ch, &mut shift, out);
        if self.is_mixed() && shift == Shift::Double {
            out.push(SI);
        }
        ok
    }

    /// The shift state data in this CCSID starts in.
    pub(crate) fn initial_shift(&self) -> Shift {
        match self.form {
            Form::Dbcs { .. } => Shift::Double,
            _ => Shift::Single,
        }
    }

    /// Decode a single-byte table's form.
    fn decode_sbcs(table: &[u32; 256], bytes: &[u8], out: &mut Vec<char>) -> usize {
        out.extend(
            bytes
                .iter()
                .map(|&b| char::from_u32(table[usize::from(b)]).unwrap_or('\u{1A}')),
        );
        bytes.len()
    }

    /// Decode `bytes` from shift state `shift`, leaving it updated.
    pub(crate) fn decode_in(&self, bytes: &[u8], shift: &mut Shift, out: &mut Vec<char>) -> usize {
        match self.form {
            Form::Sbcs { table, .. } => Self::decode_sbcs(table, bytes, out),
            Form::Mixed { sbcs, dbcs } => mixed::decode(sbcs, dbcs, bytes, shift, out),
            Form::Dbcs { dbcs } => mixed::decode_pairs(dbcs, bytes, out),
            Form::Utf8 => unicode::decode_utf8(bytes, out),
            Form::Utf16 => unicode::decode_utf16(bytes, out),
        }
    }

    /// Encode one character from output shift state `shift`, leaving
    /// it updated; returns whether it was encoded exactly.
    pub(crate) fn encode_in(&self, ch: char, shift: &mut Shift, out: &mut Vec<u8>) -> bool {
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
            Form::Mixed { dbcs, .. } => mixed::encode(reverse(self.ccsid), dbcs, ch, shift, out),
            Form::Dbcs { dbcs } => {
                let p = dbcs.encode(ch);
                out.extend_from_slice(&p.unwrap_or(DBCS_SUB).to_be_bytes());
                p.is_some()
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

/// The reverse map of a CCSID's single-byte table (a mixed CCSID's
/// single-byte half), built once per CCSID. Keyed by CCSID: a table's
/// address is not stable when it is a constant.
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
        for (c, t) in [(930, &SB0930), (5026, &SB0930), (939, &SB0939), (5035, &SB0939)] {
            m.insert(c, of(t));
        }
        m
    });
    &all[&ccsid]
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
mod tests;
