// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names with their special values, as commands take them.

use std::fmt;
use std::str::FromStr;

use crate::{Name, NameError};

/// A library parameter: a name, `*LIBL` or `*CURLIB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LibSpec {
    /// A named library.
    Name(Name),
    /// Search the library list.
    Libl,
    /// The current library.
    Curlib,
}

/// A member parameter: a name, `*ALL`, `*FIRST` or `*LAST`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberSpec {
    /// A named member.
    Name(Name),
    /// Every member.
    All,
    /// The first member.
    First,
    /// The last member.
    Last,
}

/// An object parameter: a name or `*ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectSpec {
    /// A named object.
    Name(Name),
    /// Every object.
    All,
}

fn special(s: &str) -> Option<String> {
    s.strip_prefix('*').map(str::to_ascii_uppercase)
}

fn bad_special(s: &str) -> NameError {
    NameError::BadSpecial {
        value: s.to_string(),
    }
}

impl FromStr for LibSpec {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match special(s).as_deref() {
            Some("LIBL") => Ok(Self::Libl),
            Some("CURLIB") => Ok(Self::Curlib),
            Some(_) => Err(bad_special(s)),
            None => Name::parse(s).map(Self::Name),
        }
    }
}

impl FromStr for MemberSpec {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match special(s).as_deref() {
            Some("ALL") => Ok(Self::All),
            Some("FIRST") => Ok(Self::First),
            Some("LAST") => Ok(Self::Last),
            Some(_) => Err(bad_special(s)),
            None => Name::parse(s).map(Self::Name),
        }
    }
}

impl FromStr for ObjectSpec {
    type Err = NameError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match special(s).as_deref() {
            Some("ALL") => Ok(Self::All),
            Some(_) => Err(bad_special(s)),
            None => Name::parse(s).map(Self::Name),
        }
    }
}

impl fmt::Display for LibSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => n.fmt(f),
            Self::Libl => f.write_str("*LIBL"),
            Self::Curlib => f.write_str("*CURLIB"),
        }
    }
}

impl fmt::Display for MemberSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => n.fmt(f),
            Self::All => f.write_str("*ALL"),
            Self::First => f.write_str("*FIRST"),
            Self::Last => f.write_str("*LAST"),
        }
    }
}

impl fmt::Display for ObjectSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name(n) => n.fmt(f),
            Self::All => f.write_str("*ALL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specials_and_names() {
        assert_eq!("*libl".parse::<LibSpec>().unwrap(), LibSpec::Libl);
        assert_eq!("*CURLIB".parse::<LibSpec>().unwrap(), LibSpec::Curlib);
        assert_eq!("qgpl".parse::<LibSpec>().unwrap().to_string(), "QGPL");
        assert!(matches!(
            "*ALL".parse::<LibSpec>(),
            Err(NameError::BadSpecial { .. })
        ));
        assert_eq!("*first".parse::<MemberSpec>().unwrap(), MemberSpec::First);
        assert_eq!("*LAST".parse::<MemberSpec>().unwrap().to_string(), "*LAST");
        assert_eq!("*all".parse::<ObjectSpec>().unwrap(), ObjectSpec::All);
        assert!(matches!(
            "*NONE".parse::<ObjectSpec>(),
            Err(NameError::BadSpecial { .. })
        ));
        assert!(matches!(
            "9".parse::<ObjectSpec>(),
            Err(NameError::BadChar { .. })
        ));
    }
}
