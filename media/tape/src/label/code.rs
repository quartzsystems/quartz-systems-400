// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Label character codes: CODE(*EBCDIC), CP037 as the system writes
//! labels, and CODE(*ASCII).

use super::LabelError;
use crate::spec::Field;

/// The code labels are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Code {
    /// CP037 EBCDIC, the default.
    #[default]
    Ebcdic,
    /// 7-bit ASCII.
    Ascii,
}

impl Code {
    /// The space character.
    #[must_use]
    pub fn space(self) -> u8 {
        match self {
            Self::Ebcdic => ccsid::SPACE,
            Self::Ascii => b' ',
        }
    }

    /// Encode `text` into `out`, padded with spaces.
    pub fn put(self, out: &mut [u8], text: &str) -> Result<(), LabelError> {
        let bad = || LabelError::Field(text.to_string());
        match self {
            Self::Ebcdic => ccsid::encode_fixed(text, out).map_err(|_| bad()),
            Self::Ascii => {
                if text.len() > out.len() || !text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
                    return Err(bad());
                }
                out[..text.len()].copy_from_slice(text.as_bytes());
                out[text.len()..].fill(b' ');
                Ok(())
            }
        }
    }

    /// Decode a field, dropping trailing spaces.
    #[must_use]
    pub fn get(self, field: &[u8]) -> String {
        match self {
            Self::Ebcdic => ccsid::decode_trimmed(field),
            Self::Ascii => {
                let s: String = field.iter().map(|&b| char::from(b)).collect();
                s.trim_end_matches(' ').to_string()
            }
        }
    }

    /// Encode `text` into field `f` of a label.
    pub fn put_field(self, rec: &mut [u8], f: Field, text: &str) -> Result<(), LabelError> {
        self.put(&mut rec[f.offset..f.offset + f.size], text)
    }

    /// A number into field `f`, zero-padded.
    pub fn put_num(self, rec: &mut [u8], f: Field, n: u64) -> Result<(), LabelError> {
        let s = format!("{n:0width$}", width = f.size);
        if s.len() > f.size {
            return Err(LabelError::Field(s));
        }
        self.put_field(rec, f, &s)
    }

    /// Field `f` of a label, trailing spaces dropped.
    #[must_use]
    pub fn get_field(self, rec: &[u8], f: Field) -> String {
        self.get(&rec[f.offset..f.offset + f.size])
    }

    /// Field `f` as a number; blanks are zero.
    pub fn get_num(self, rec: &[u8], f: Field) -> Result<u64, LabelError> {
        let s = self.get_field(rec, f);
        let s = s.trim();
        if s.is_empty() {
            return Ok(0);
        }
        s.parse().map_err(|_| LabelError::Field(s.to_string()))
    }

    /// The code a label was written in, from its first four bytes.
    #[must_use]
    pub fn detect(rec: &[u8], ident: &str) -> Option<Self> {
        let head = rec.get(..ident.len())?;
        [Self::Ebcdic, Self::Ascii]
            .into_iter()
            .find(|c| c.get(head) == ident)
    }
}
