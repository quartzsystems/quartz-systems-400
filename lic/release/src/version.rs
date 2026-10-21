// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `VxRyMz`.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// A version, release and modification level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Release {
    /// Version: a new generation of the OS.
    pub v: u8,
    /// Release: adds function; may change on-disk formats.
    pub r: u8,
    /// Modification: maintenance only; never changes formats.
    pub m: u8,
}

/// A string that is not `VxRyMz`.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("not a release identifier (want VxRyMz): {0}")]
pub struct ParseReleaseError(pub String);

impl Release {
    /// Same V and R: the on-disk formats are compatible. M never changes
    /// them. On R0 formats may still move without an R bump; `store`
    /// records the creating release and refuses a different V/R.
    pub fn is_compatible_store(self, other: Release) -> bool {
        self.v == other.v && self.r == other.r
    }

    /// `V1R0M0 build a1b2c3d`.
    pub fn full(self) -> String {
        format!("{self} build {}", crate::build_id())
    }
}

impl fmt::Display for Release {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V{}R{}M{}", self.v, self.r, self.m)
    }
}

impl FromStr for Release {
    type Err = ParseReleaseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseReleaseError(s.to_string());
        let rest = s.strip_prefix('V').ok_or_else(err)?;
        let (v, rest) = rest.split_once('R').ok_or_else(err)?;
        let (r, m) = rest.split_once('M').ok_or_else(err)?;
        let num = |t: &str| {
            if t.is_empty() || t.len() > 3 || !t.bytes().all(|b| b.is_ascii_digit()) {
                return Err(err());
            }
            t.parse::<u8>().map_err(|_| err())
        };
        Ok(Self {
            v: num(v)?,
            r: num(r)?,
            m: num(m)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_parse() {
        let r: Release = "V1R0M0".parse().unwrap();
        assert_eq!(r, Release { v: 1, r: 0, m: 0 });
        assert_eq!(r.to_string(), "V1R0M0");
        assert_eq!("V7R4M0".parse::<Release>().unwrap().to_string(), "V7R4M0");
        for bad in ["1R0M0", "V1R0", "V1RxM0", "v1r0m0", "V1R0M0x", "V1000R0M0"] {
            assert!(bad.parse::<Release>().is_err(), "{bad}");
        }
    }

    #[test]
    fn ordering_is_v_then_r_then_m() {
        let p = |s: &str| s.parse::<Release>().unwrap();
        assert!(p("V1R0M0") < p("V1R0M1"));
        assert!(p("V1R0M9") < p("V1R1M0"));
        assert!(p("V1R9M9") < p("V2R0M0"));
    }

    #[test]
    fn store_compatibility() {
        let p = |s: &str| s.parse::<Release>().unwrap();
        assert!(p("V1R1M0").is_compatible_store(p("V1R1M3")));
        assert!(!p("V1R1M0").is_compatible_store(p("V1R2M0")));
        assert!(!p("V1R1M0").is_compatible_store(p("V2R1M0")));
    }
}
