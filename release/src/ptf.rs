// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTF and cumulative package identifiers. Defined now; nothing consumes
//! them until later phases (DSPPTF, GO LICPGM, QSZRTVPR: `TODO(phase11)`).

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A program temporary fix: `QS` plus five digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ptf(pub u32);

/// A cumulative PTF package: `C` + `yyddd` (Julian date) + `110`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cumulative {
    /// Two-digit year.
    pub year: u8,
    /// Day of year, 1 to 366.
    pub day: u16,
}

/// Not a valid identifier.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("not a {kind} identifier: {text}")]
pub struct ParseIdError {
    /// `PTF` or `cumulative package`.
    pub kind: &'static str,
    /// The rejected text.
    pub text: String,
}

impl fmt::Display for Ptf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QS{:05}", self.0)
    }
}

impl FromStr for Ptf {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseIdError {
            kind: "PTF",
            text: s.to_string(),
        };
        let digits = s.strip_prefix("QS").ok_or_else(err)?;
        if digits.len() != 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        digits.parse().map(Ptf).map_err(|_| err())
    }
}

impl fmt::Display for Cumulative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{:02}{:03}110", self.year, self.day)
    }
}

impl FromStr for Cumulative {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseIdError {
            kind: "cumulative package",
            text: s.to_string(),
        };
        let body = s
            .strip_prefix('C')
            .and_then(|t| t.strip_suffix("110"))
            .ok_or_else(err)?;
        if body.len() != 5 || !body.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        let year = body[..2].parse().map_err(|_| err())?;
        let day: u16 = body[2..].parse().map_err(|_| err())?;
        if day == 0 || day > 366 {
            return Err(err());
        }
        Ok(Self { year, day })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ptf_round_trip() {
        assert_eq!(Ptf(1).to_string(), "QS00001");
        assert_eq!("QS12345".parse::<Ptf>().unwrap(), Ptf(12345));
        for bad in ["QS1", "SI12345", "QS123456", "QS1234x"] {
            assert!(bad.parse::<Ptf>().is_err(), "{bad}");
        }
    }

    #[test]
    fn cumulative_round_trip() {
        let c = Cumulative { year: 26, day: 283 };
        assert_eq!(c.to_string(), "C26283110");
        assert_eq!("C26283110".parse::<Cumulative>().unwrap(), c);
        for bad in ["C26283", "C26367110", "C26000110", "X26283110", "C2628311"] {
            assert!(bad.parse::<Cumulative>().is_err(), "{bad}");
        }
        assert!(Cumulative { year: 25, day: 300 } < c);
    }
}
