// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetBIOS` names (RFC 1001/1002): 15 characters and a suffix byte, in the
//! first-level ("half-ASCII") encoding with an optional scope.

use super::NbssError;

/// The server service suffix.
pub const SUFFIX_SERVER: u8 = 0x20;
/// The workstation service suffix.
pub const SUFFIX_WORKSTATION: u8 = 0x00;

/// A decoded `NetBIOS` name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbName {
    /// The name, trailing spaces removed.
    pub name: String,
    /// The 16th byte.
    pub suffix: u8,
    /// The scope, dotted (empty for none).
    pub scope: String,
}

impl NbName {
    /// A name with no scope; `name` is upper-cased and cut to 15 bytes.
    pub fn new(name: &str, suffix: u8) -> Self {
        let mut n: String = name.to_ascii_uppercase();
        n.truncate(15);
        Self {
            name: n,
            suffix,
            scope: String::new(),
        }
    }

    /// Whether this is `name` (case-insensitive) with `suffix`.
    pub fn is(&self, name: &str, suffix: u8) -> bool {
        self.suffix == suffix && self.name.eq_ignore_ascii_case(name)
    }

    /// The encoded form: length 32, the half-ASCII bytes, scope labels, 0.
    /// The wildcard `*` (node status, RFC 1002 4.2.17) is padded with
    /// zero bytes, every other name with spaces.
    pub fn encode(&self) -> Vec<u8> {
        let pad = if self.name == "*" { 0 } else { b' ' };
        let mut raw = [pad; 16];
        for (d, s) in raw.iter_mut().zip(self.name.bytes().take(15)) {
            *d = s;
        }
        raw[15] = self.suffix;
        let mut out = vec![32u8];
        for b in raw {
            out.push(b'A' + (b >> 4));
            out.push(b'A' + (b & 0x0f));
        }
        for label in self.scope.split('.').filter(|l| !l.is_empty()) {
            let l = &label.as_bytes()[..label.len().min(63)];
            out.push(u8::try_from(l.len()).expect("63 at most"));
            out.extend_from_slice(l);
        }
        out.push(0);
        out
    }

    /// Decodes a name at the start of `b`; returns it and the bytes used.
    /// Compression pointers (name service only) are not followed.
    pub fn decode(b: &[u8]) -> Result<(Self, usize), NbssError> {
        if b.first() != Some(&32) || b.len() < 34 {
            return Err(NbssError::BadName);
        }
        let mut raw = [0u8; 16];
        for (i, r) in raw.iter_mut().enumerate() {
            let hi = b[1 + 2 * i].wrapping_sub(b'A');
            let lo = b[2 + 2 * i].wrapping_sub(b'A');
            if hi > 15 || lo > 15 {
                return Err(NbssError::BadName);
            }
            *r = (hi << 4) | lo;
        }
        let mut pos = 33;
        let mut labels = Vec::new();
        loop {
            let n = usize::from(*b.get(pos).ok_or(NbssError::BadName)?);
            pos += 1;
            if n == 0 {
                break;
            }
            if n > 63 {
                return Err(NbssError::BadName);
            }
            let l = b.get(pos..pos + n).ok_or(NbssError::BadName)?;
            labels.push(String::from_utf8_lossy(l).into_owned());
            pos += n;
        }
        let mut name = String::from_utf8_lossy(&raw[..15])
            .trim_end_matches(' ')
            .to_string();
        if name.starts_with('*') {
            name = name.trim_end_matches(char::from(0)).to_string();
        }
        Ok((
            Self {
                name,
                suffix: raw[15],
                scope: labels.join("."),
            },
            pos,
        ))
    }
}
