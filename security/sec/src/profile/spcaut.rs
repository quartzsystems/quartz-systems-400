// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Special authorities and user classes.

/// A set of special authorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SpecialAuth(pub u32);

impl SpecialAuth {
    /// `*ALLOBJ`.
    pub const ALLOBJ: Self = Self(0x01);
    /// `*SECADM`.
    pub const SECADM: Self = Self(0x02);
    /// `*JOBCTL`.
    pub const JOBCTL: Self = Self(0x04);
    /// `*SPLCTL`.
    pub const SPLCTL: Self = Self(0x08);
    /// `*SAVSYS`.
    pub const SAVSYS: Self = Self(0x10);
    /// `*SERVICE`.
    pub const SERVICE: Self = Self(0x20);
    /// `*AUDIT`.
    pub const AUDIT: Self = Self(0x40);
    /// `*IOSYSCFG`.
    pub const IOSYSCFG: Self = Self(0x80);
    /// None.
    pub const NONE: Self = Self(0);
    /// All eight.
    pub const ALL: Self = Self(0xFF);

    /// Names in IBM's display order.
    pub const NAMES: [(&'static str, Self); 8] = [
        ("*ALLOBJ", Self::ALLOBJ),
        ("*SECADM", Self::SECADM),
        ("*JOBCTL", Self::JOBCTL),
        ("*SPLCTL", Self::SPLCTL),
        ("*SAVSYS", Self::SAVSYS),
        ("*SERVICE", Self::SERVICE),
        ("*AUDIT", Self::AUDIT),
        ("*IOSYSCFG", Self::IOSYSCFG),
    ];

    /// Whether every authority in `other` is here.
    #[must_use]
    pub fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Both sets.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// This set without `other`.
    #[must_use]
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The names, `*NONE` for none.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        let v: Vec<&str> = Self::NAMES
            .iter()
            .filter(|(_, a)| self.has(*a))
            .map(|(n, _)| *n)
            .collect();
        if v.is_empty() { vec!["*NONE"] } else { v }
    }

    /// One special authority by name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, a)| *a)
    }

    /// Parse a list of names; `*NONE` alone is the empty set.
    pub fn parse(names: &[&str]) -> Result<Self, String> {
        let mut s = Self::NONE;
        for n in names {
            if n.eq_ignore_ascii_case("*NONE") && names.len() == 1 {
                return Ok(Self::NONE);
            }
            s = s.union(Self::by_name(n).ok_or_else(|| format!("{n} is not a special authority"))?);
        }
        Ok(s)
    }
}

/// `USRCLS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum UserClass {
    /// `*USER`.
    #[default]
    User,
    /// `*SYSOPR`.
    SysOpr,
    /// `*PGMR`.
    Pgmr,
    /// `*SECADM`.
    SecAdm,
    /// `*SECOFR`.
    SecOfr,
}

impl UserClass {
    /// Every class, by code.
    pub const ALL: [Self; 5] = [
        Self::User,
        Self::SysOpr,
        Self::Pgmr,
        Self::SecAdm,
        Self::SecOfr,
    ];

    /// The stored code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::User => 0,
            Self::SysOpr => 1,
            Self::Pgmr => 2,
            Self::SecAdm => 3,
            Self::SecOfr => 4,
        }
    }

    /// From the stored code.
    #[must_use]
    pub fn from_code(c: u8) -> Option<Self> {
        Self::ALL.get(usize::from(c)).copied()
    }

    /// `*USER` ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::User => "*USER",
            Self::SysOpr => "*SYSOPR",
            Self::Pgmr => "*PGMR",
            Self::SecAdm => "*SECADM",
            Self::SecOfr => "*SECOFR",
        }
    }

    /// By name.
    #[must_use]
    pub fn by_name(n: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(n))
    }

    /// `SPCAUT(*USRCLS)`: IBM's defaults for the class at a security
    /// level. At 10 and 20 every class also has `*ALLOBJ` and `*SAVSYS`.
    #[must_use]
    pub fn defaults(self, qsecurity: u8) -> SpecialAuth {
        use SpecialAuth as S;
        let low = qsecurity < 30;
        match self {
            Self::SecOfr => S::ALL,
            Self::SecAdm if low => S::ALLOBJ.union(S::SAVSYS).union(S::SECADM).union(S::JOBCTL),
            Self::SecAdm => S::SECADM,
            Self::Pgmr | Self::SysOpr if low => S::ALLOBJ.union(S::SAVSYS).union(S::JOBCTL),
            Self::SysOpr => S::SAVSYS.union(S::JOBCTL),
            Self::User if low => S::ALLOBJ.union(S::SAVSYS),
            Self::Pgmr | Self::User => S::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_defaults() {
        let s = SpecialAuth::parse(&["*jobctl", "*SAVSYS"]).unwrap();
        assert_eq!(s.names(), vec!["*JOBCTL", "*SAVSYS"]);
        assert_eq!(SpecialAuth::parse(&["*NONE"]).unwrap(), SpecialAuth::NONE);
        assert!(SpecialAuth::parse(&["*BOGUS"]).is_err());
        assert_eq!(UserClass::SysOpr.defaults(40), s);
        assert!(UserClass::User.defaults(20).has(SpecialAuth::ALLOBJ));
        assert_eq!(UserClass::User.defaults(30), SpecialAuth::NONE);
        assert_eq!(UserClass::SecOfr.defaults(50).names().len(), 8);
        assert_eq!(UserClass::from_code(4), Some(UserClass::SecOfr));
        assert_eq!(UserClass::by_name("*pgmr"), Some(UserClass::Pgmr));
    }
}
