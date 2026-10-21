// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service program signatures: 16 bytes that a program bound to the
//! service program checks at activation. Generated signatures are the
//! first 16 bytes of the SHA-256 of the ordered export names, each
//! followed by a newline; explicit ones are the binder source's text,
//! CP037 and blank padded, or 32 hexadecimal digits.

use std::fmt;

use sha2::{Digest, Sha256};

use crate::error::{IleError, Result};

/// A 16-byte signature.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature(pub [u8; 16]);

impl Signature {
    /// The generated signature of an ordered export list.
    #[must_use]
    pub fn of_exports(names: &[String]) -> Self {
        let mut h = Sha256::new();
        for n in names {
            h.update(n.as_bytes());
            h.update(b"\n");
        }
        let d = h.finalize();
        let mut out = [0u8; 16];
        out.copy_from_slice(&d[..16]);
        Self(out)
    }

    /// An explicit signature from binder source: text of up to 16
    /// characters (CP037, blank padded) or `X'…'` with 32 hex digits.
    pub fn from_source(text: &str, hex: bool) -> Result<Self> {
        let mut out = [0u8; 16];
        if hex {
            let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if t.len() != 32 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(IleError::Signature(text.into()));
            }
            for i in 0..16 {
                out[i] = u8::from_str_radix(&t[2 * i..2 * i + 2], 16)
                    .map_err(|_| IleError::Signature(text.into()))?;
            }
            return Ok(Self(out));
        }
        if text.is_empty() || text.chars().count() > 16 {
            return Err(IleError::Signature(text.into()));
        }
        out.fill(ccsid::SPACE);
        ccsid::encode_fixed(text, &mut out).map_err(|_| IleError::Signature(text.into()))?;
        Ok(Self(out))
    }

    /// The signature as 32 hexadecimal digits.
    #[must_use]
    pub fn hex(&self) -> String {
        use std::fmt::Write as _;
        self.0.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02X}");
            s
        })
    }
}

/// The generated signature of an ordered export list.
#[must_use]
pub fn signature_of(names: &[String]) -> Signature {
    Signature::of_exports(names)
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", self.hex())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_signatures_depend_on_order() {
        let a = Signature::of_exports(&["A".into(), "B".into()]);
        let b = Signature::of_exports(&["B".into(), "A".into()]);
        let c = Signature::of_exports(&["A".into(), "B".into()]);
        assert_ne!(a, b);
        assert_eq!(a, c);
        assert_eq!(a.hex().len(), 32);
    }

    #[test]
    fn explicit_signatures() {
        let s = Signature::from_source("MYSIG", false).unwrap();
        assert_eq!(&s.0[..5], &[0xD4, 0xE8, 0xE2, 0xC9, 0xC7]);
        assert_eq!(s.0[5], 0x40);
        let h = Signature::from_source("000102030405060708090A0B0C0D0E0F", true).unwrap();
        assert_eq!(h.0[15], 15);
        assert!(Signature::from_source("ABC", true).is_err());
        assert!(Signature::from_source("SEVENTEEN CHARS!!", false).is_err());
    }
}
