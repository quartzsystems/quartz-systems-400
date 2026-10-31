// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Product identifiers, options, load types and load IDs.

use std::fmt;
use std::str::FromStr;

use release::Release;
use thiserror::Error;

/// A value that is not a valid identifier of its kind.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("not a valid {kind}: {text:?}")]
pub struct IdError {
    /// What was being parsed.
    pub kind: &'static str,
    /// The rejected text.
    pub text: String,
}

fn bad(kind: &'static str, text: &str) -> IdError {
    IdError {
        kind,
        text: text.to_string(),
    }
}

/// A seven-character product ID: IBM's `nxxxxxx` (a digit, then digits or
/// letters) or Quartz's `QSxxxxx`, reserved for Quartz.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProductId(String);

impl ProductId {
    /// The operating system.
    pub const OS: &'static str = "QS00SS1";

    /// The ID as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the ID is in the Quartz-reserved `QS` range.
    #[must_use]
    pub fn is_quartz(&self) -> bool {
        self.0.starts_with("QS")
    }
}

impl FromStr for ProductId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let b = s.as_bytes();
        let alnum = |c: &u8| c.is_ascii_digit() || c.is_ascii_uppercase();
        let ok =
            b.len() == 7 && b[1..].iter().all(alnum) && (b[0].is_ascii_digit() || &b[..2] == b"QS");
        if ok {
            Ok(Self(s.to_string()))
        } else {
            Err(bad("product ID", s))
        }
    }
}

impl fmt::Display for ProductId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A product option: 0 is `*BASE`, then 1 to 99.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OptionNo(pub u8);

impl OptionNo {
    /// The base option.
    pub const BASE: OptionNo = OptionNo(0);

    /// Whether this is `*BASE`.
    #[must_use]
    pub fn is_base(self) -> bool {
        self.0 == 0
    }

    /// The four-digit form the APIs use: `0000`, `0033`.
    #[must_use]
    pub fn api(self) -> String {
        format!("{:04}", self.0)
    }
}

impl FromStr for OptionNo {
    type Err = IdError;

    /// `*BASE`, `0000`..`0099` or `1`..`99`.
    fn from_str(s: &str) -> Result<Self, IdError> {
        if s.eq_ignore_ascii_case("*BASE") {
            return Ok(Self::BASE);
        }
        if s.is_empty() || s.len() > 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad("product option", s));
        }
        match s.parse::<u8>() {
            Ok(n) if n <= 99 => Ok(Self(n)),
            _ => Err(bad("product option", s)),
        }
    }
}

impl fmt::Display for OptionNo {
    /// `*BASE` or the bare number, as the displays show it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_base() {
            f.write_str("*BASE")
        } else {
            write!(f, "{}", self.0)
        }
    }
}

/// `*CODE` or `*LNG`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadType {
    /// The program objects.
    Code,
    /// The national-language objects of one feature.
    Lng,
}

impl LoadType {
    /// `*CODE` or `*LNG`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            LoadType::Code => "*CODE",
            LoadType::Lng => "*LNG",
        }
    }

    /// Parse `*CODE` or `*LNG`.
    pub fn parse(s: &str) -> Result<Self, IdError> {
        match s.to_ascii_uppercase().as_str() {
            "*CODE" => Ok(LoadType::Code),
            "*LNG" => Ok(LoadType::Lng),
            _ => Err(bad("load type", s)),
        }
    }
}

/// A four-character load ID: a feature code `5001`..`9999` for a code
/// load, a language feature (`2924`) for a language load.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LoadId(String);

impl LoadId {
    /// The default code load ID.
    pub const CODE_DEFAULT: &'static str = "5001";
    /// English uppercase and lowercase, the shipped language.
    pub const ENGLISH: &'static str = "2924";

    /// The ID as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A feature code for a code load.
    pub fn feature(s: &str) -> Result<Self, IdError> {
        match s.parse::<u16>() {
            Ok(n) if s.len() == 4 && (5001..=9999).contains(&n) => Ok(Self(s.to_string())),
            _ => Err(bad("code load ID", s)),
        }
    }

    /// A national language version (`29xx`) for a language load.
    pub fn nlv(s: &str) -> Result<Self, IdError> {
        if s.len() == 4 && s.starts_with("29") && s.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(s.to_string()))
        } else {
            Err(bad("language load ID", s))
        }
    }

    /// Either kind, chosen by the load type.
    pub fn for_type(t: LoadType, s: &str) -> Result<Self, IdError> {
        match t {
            LoadType::Code => Self::feature(s),
            LoadType::Lng => Self::nlv(s),
        }
    }
}

impl fmt::Display for LoadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Parse a release in `VxRyMz` or the APIs' `vvrrmm` form (`010000`).
pub fn parse_release(s: &str) -> Result<Release, IdError> {
    if let Ok(r) = s.parse::<Release>() {
        return Ok(r);
    }
    let b = s.as_bytes();
    if b.len() == 6 && b[..4].iter().all(u8::is_ascii_digit) {
        let v: u8 = s[..2].parse().map_err(|_| bad("release", s))?;
        let r: u8 = s[2..4].parse().map_err(|_| bad("release", s))?;
        let m = match b[4..6] {
            [b'0', d] if d.is_ascii_digit() => d - b'0',
            [b'0', d] if d.is_ascii_uppercase() => d - b'A' + 10,
            _ => return Err(bad("release", s)),
        };
        if v <= 35 && r <= 35 {
            return Ok(Release { v, r, m });
        }
    }
    Err(bad("release", s))
}

/// The six-character form of a release: `VxRyMz` while every part is
/// one digit, else `vvrrmm`.
#[must_use]
pub fn release_text(r: Release) -> String {
    if r.v < 10 && r.r < 10 && r.m < 10 {
        r.to_string()
    } else {
        let m = if r.m < 10 {
            char::from(b'0' + r.m)
        } else {
            char::from(b'A' + r.m - 10)
        };
        format!("{:02}{:02}0{m}", r.v, r.r)
    }
}

/// A license term: `V1`, `V1R0` or `V1R0M0`, the span a license key
/// or usage limit covers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LicTerm(String);

impl LicTerm {
    /// The term as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The version term covering a release (`V1` for `V1R0M0`).
    #[must_use]
    pub fn version_of(r: Release) -> Self {
        Self(format!("V{}", r.v))
    }

    /// Whether a release falls within the term.
    #[must_use]
    pub fn covers(&self, r: Release) -> bool {
        let full = r.to_string();
        full.starts_with(&self.0)
            && matches!(full.as_bytes().get(self.0.len()), None | Some(b'R' | b'M'))
    }
}

impl FromStr for LicTerm {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let rest = s.strip_prefix('V').ok_or_else(|| bad("license term", s))?;
        let ok = match rest.split_once('R') {
            None => !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()),
            Some(_) => s.parse::<Release>().is_ok() || format!("{s}M0").parse::<Release>().is_ok(),
        };
        if ok && s.len() <= 6 {
            Ok(Self(s.to_string()))
        } else {
            Err(bad("license term", s))
        }
    }
}

impl fmt::Display for LicTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether text is a valid object or library name (unquoted form):
/// one to ten of `A-Z 0-9 $ # @ _ .`, not starting with a digit,
/// `_` or `.`.
#[must_use]
pub fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 10
        && (b[0].is_ascii_uppercase() || matches!(b[0], b'$' | b'#' | b'@'))
        && b.iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || b"$#@_.".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_ids() {
        for ok in ["QS00SS1", "5770SS1", "1AB1234", "5770ABC", "QS00WDS"] {
            assert!(ok.parse::<ProductId>().is_ok(), "{ok}");
        }
        for no in [
            "QS0SS1", "A5770SS", "57a0SS1", "5770ss1", "XS00SS1", "5770SS12",
        ] {
            assert!(no.parse::<ProductId>().is_err(), "{no}");
        }
        assert!("QS00SS1".parse::<ProductId>().unwrap().is_quartz());
    }

    #[test]
    fn options_and_loads() {
        assert_eq!("*BASE".parse::<OptionNo>().unwrap(), OptionNo::BASE);
        assert_eq!("0033".parse::<OptionNo>().unwrap(), OptionNo(33));
        assert_eq!(OptionNo(33).api(), "0033");
        assert!("100".parse::<OptionNo>().is_err());
        assert!(LoadId::feature("5001").is_ok());
        assert!(LoadId::feature("2924").is_err());
        assert!(LoadId::nlv("2924").is_ok());
        assert!(LoadId::nlv("5001").is_err());
    }

    #[test]
    fn releases_and_terms() {
        let r = parse_release("V1R0M0").unwrap();
        assert_eq!(parse_release("010000").unwrap(), r);
        assert_eq!(release_text(r), "V1R0M0");
        let big = Release { v: 11, r: 3, m: 10 };
        assert_eq!(release_text(big), "11030A");
        assert_eq!(parse_release("11030A").unwrap(), big);
        let t: LicTerm = "V1".parse().unwrap();
        assert!(t.covers(r));
        assert!(!"V11".parse::<LicTerm>().unwrap().covers(r));
        assert!("V1R0".parse::<LicTerm>().unwrap().covers(r));
        assert!("X1".parse::<LicTerm>().is_err());
    }

    #[test]
    fn names() {
        assert!(is_name("QSYS"));
        assert!(is_name("SS100C5050"));
        assert!(!is_name("1ABC"));
        assert!(!is_name("TOOLONGNAME1"));
    }
}
