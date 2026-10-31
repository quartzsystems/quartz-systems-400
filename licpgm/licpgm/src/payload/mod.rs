// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object payloads: `*PRDDFN` and `*PRDLOD`, plus the license and
//! agreement records the integrator keeps beside them.

pub mod codec;
pub mod prddfn;
pub mod prdlod;
pub mod spec;

use thiserror::Error;

/// A payload that does not decode, or a value that does not encode.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PayloadError {
    /// Ends before byte `0`.
    #[error("payload ends before byte {0}")]
    Short(usize),
    /// Wrong magic.
    #[error("payload magic is wrong")]
    Magic,
    /// Unknown format version.
    #[error("payload format version {0} is not known")]
    Version(u16),
    /// A field holds a value it may not.
    #[error("payload field {0} is not valid")]
    Bad(&'static str),
    /// Text that cannot be stored in the field.
    #[error("text of {0} cannot be stored")]
    Text(&'static str),
    /// More entries than the count field holds.
    #[error("too many entries in {0}")]
    TooMany(&'static str),
    /// Bytes after the last section.
    #[error("{0} bytes after the payload")]
    Trailing(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn shipped_payloads_round_trip() {
        let cat = catalog::shipped();
        for p in &cat.products {
            let b = prddfn::encode(&p.definition).unwrap();
            assert_eq!(prddfn::decode(&b).unwrap(), p.definition);
            for (_, l) in &p.loads {
                let b = prdlod::encode(l).unwrap();
                assert_eq!(&prdlod::decode(&b).unwrap(), l);
            }
        }
    }

    #[test]
    fn damaged_payloads_are_refused() {
        let cat = catalog::shipped();
        let d = &cat.products[0].definition;
        let mut b = prddfn::encode(d).unwrap();
        assert!(matches!(
            prddfn::decode(&b[..100]),
            Err(PayloadError::Short(_))
        ));
        b.push(0);
        assert!(matches!(prddfn::decode(&b), Err(PayloadError::Trailing(1))));
        b[0] = b'X';
        assert_eq!(prddfn::decode(&b), Err(PayloadError::Magic));
        let (_, l) = &cat.products[0].loads[0];
        let mut b = prdlod::encode(l).unwrap();
        b[8] = 9;
        assert!(matches!(prdlod::decode(&b), Err(PayloadError::Version(_))));
    }
}
