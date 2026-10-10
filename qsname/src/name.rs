// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 10-byte name.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use crate::NameError;

/// Bytes in a stored name.
pub const NAME_LEN: usize = 10;

/// A validated name in its stored form: 10 CP037 bytes, space padded.
/// Ordering is EBCDIC byte order, which is what listings use.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name([u8; NAME_LEN]);

impl Name {
    /// Parse and normalize.
    pub fn parse(s: &str) -> Result<Self, NameError> {
        let too_long = || NameError::TooLong {
            name: s.to_string(),
        };
        let bad = |ch: char| NameError::BadChar {
            name: s.to_string(),
            ch,
        };
        if s.is_empty() {
            return Err(NameError::Empty);
        }
        let text: String = if let Some(inner) = s.strip_prefix('"') {
            let Some(inner) = inner.strip_suffix('"') else {
                return Err(NameError::Unclosed {
                    name: s.to_string(),
                });
            };
            if inner.is_empty() {
                return Err(NameError::Empty);
            }
            if inner.chars().count() > 8 {
                return Err(too_long());
            }
            for ch in inner.chars() {
                if ch == '"' || ch.is_control() || !ccsid::is_representable(ch) {
                    return Err(bad(ch));
                }
            }
            s.to_string()
        } else {
            if s.chars().count() > NAME_LEN {
                return Err(too_long());
            }
            let upper = s.to_ascii_uppercase();
            let mut chars = upper.chars();
            let first = chars.next().expect("non-empty");
            if !(first.is_ascii_uppercase() || "$#@".contains(first)) {
                return Err(bad(first));
            }
            for ch in chars {
                if !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || "$#@_".contains(ch)) {
                    return Err(bad(ch));
                }
            }
            upper
        };
        let mut bytes = [ccsid::SPACE; NAME_LEN];
        ccsid::encode_fixed(&text, &mut bytes).map_err(|_| too_long())?;
        Ok(Self(bytes))
    }

    /// From stored bytes, trusted (the store wrote them).
    #[must_use]
    pub fn from_bytes(bytes: [u8; NAME_LEN]) -> Self {
        Self(bytes)
    }

    /// The stored bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; NAME_LEN] {
        &self.0
    }

    /// Whether the name is quoted.
    #[must_use]
    pub fn is_quoted(&self) -> bool {
        self.0[0] == 0x7F
    }

    /// Collate like a listing.
    #[must_use]
    pub fn ebcdic_cmp(&self, other: &Self) -> Ordering {
        ccsid::ebcdic_cmp(&self.0, &other.0)
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ebcdic_cmp(other)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&ccsid::decode_trimmed(&self.0))
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Name({self})")
    }
}

impl FromStr for Name {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquoted_rules() {
        assert_eq!(Name::parse("qsys").unwrap().to_string(), "QSYS");
        assert_eq!(Name::parse("$A_1#@").unwrap().to_string(), "$A_1#@");
        assert_eq!(Name::parse("ABCDEFGHIJ").unwrap().to_string(), "ABCDEFGHIJ");
        assert!(matches!(
            Name::parse("ABCDEFGHIJK"),
            Err(NameError::TooLong { .. })
        ));
        assert!(matches!(
            Name::parse("1ABC"),
            Err(NameError::BadChar { ch: '1', .. })
        ));
        assert!(matches!(
            Name::parse("_ABC"),
            Err(NameError::BadChar { ch: '_', .. })
        ));
        assert!(matches!(
            Name::parse("A-B"),
            Err(NameError::BadChar { ch: '-', .. })
        ));
        assert!(matches!(
            Name::parse("A B"),
            Err(NameError::BadChar { ch: ' ', .. })
        ));
        assert!(matches!(Name::parse(""), Err(NameError::Empty)));
        assert_eq!(
            Name::parse("A").unwrap().as_bytes(),
            &[0xC1, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x40]
        );
    }

    #[test]
    fn quoted_rules() {
        let n = Name::parse("\"my obj\"").unwrap();
        assert!(n.is_quoted());
        assert_eq!(n.to_string(), "\"my obj\"");
        assert_eq!(n.as_bytes()[0], 0x7F);
        assert!(matches!(
            Name::parse("\"abcdefghi\""),
            Err(NameError::TooLong { .. })
        ));
        assert!(matches!(Name::parse("\"\""), Err(NameError::Empty)));
        assert!(matches!(
            Name::parse("\"abc"),
            Err(NameError::Unclosed { .. })
        ));
        assert!(matches!(
            Name::parse("\"a\"b\""),
            Err(NameError::BadChar { ch: '"', .. })
        ));
        assert!(matches!(
            Name::parse("\"a€\""),
            Err(NameError::BadChar { ch: '€', .. })
        ));
        assert!(Name::parse("\"é\"").is_ok());
    }

    #[test]
    fn ordering_is_ebcdic() {
        let mut v: Vec<Name> = ["QGPL", "$A", "A1", "#B", "A", "Z9"]
            .iter()
            .map(|s| Name::parse(s).unwrap())
            .collect();
        v.sort();
        let got: Vec<String> = v.iter().map(ToString::to_string).collect();
        assert_eq!(got, vec!["$A", "#B", "A", "A1", "QGPL", "Z9"]);
        assert_eq!(Name::parse("abc").unwrap(), Name::parse("ABC").unwrap());
        assert_eq!(
            Name::from_bytes(*Name::parse("X").unwrap().as_bytes()).to_string(),
            "X"
        );
    }
}
