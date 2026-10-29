// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Authority sets: the object and data authorities, the IBM
//! combinations they expand to, and their names both ways.

use store::obj::header::Authority as A;

/// Object and data authorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Auth(pub u32);

impl ipc::Wire for Auth {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut ipc::Dec<'_>) -> Result<Self, ipc::WireError> {
        u32::get(d).map(Self)
    }
}

/// Every single authority with its name, object authorities first.
pub const NAMES: [(&str, u32); 11] = [
    ("*OBJOPR", A::OBJOPR),
    ("*OBJMGT", A::OBJMGT),
    ("*OBJEXIST", A::OBJEXIST),
    ("*OBJALTER", A::OBJALTER),
    ("*OBJREF", A::OBJREF),
    ("*AUTLMGT", A::AUTLMGT),
    ("*READ", A::READ),
    ("*ADD", A::ADD),
    ("*UPD", A::UPD),
    ("*DLT", A::DLT),
    ("*EXECUTE", A::EXECUTE),
];

impl Auth {
    /// `*EXCLUDE`.
    pub const EXCLUDE: Self = Self(0);
    /// `*USE`.
    pub const USE: Self = Self(A::OBJOPR | A::READ | A::EXECUTE);
    /// `*CHANGE`.
    pub const CHANGE: Self = Self(A::OBJOPR | A::READ | A::ADD | A::UPD | A::DLT | A::EXECUTE);
    /// `*ALL` for any object but an authorization list.
    pub const ALL: Self = Self(0x03FF);
    /// `*ALL` for an authorization list: with `*AUTLMGT`.
    pub const ALL_AUTL: Self = Self(0x07FF);
    /// Every bit any object can carry.
    pub const MASK: u32 = 0x07FF;

    /// The store's bits.
    #[must_use]
    pub fn store(self) -> A {
        A(self.0)
    }

    /// From the store's bits, without the from-list flag.
    #[must_use]
    pub fn of(a: A) -> Self {
        Self(a.0 & Self::MASK)
    }

    /// Whether every authority of `need` is here.
    #[must_use]
    pub fn has(self, need: Self) -> bool {
        self.0 & need.0 == need.0
    }

    /// Both sets.
    #[must_use]
    pub fn union(self, o: Self) -> Self {
        Self(self.0 | o.0)
    }

    /// The authorities of `need` missing here.
    #[must_use]
    pub fn missing(self, need: Self) -> Self {
        Self(need.0 & !self.0)
    }

    /// The combination name when the bits are one (`*ALL` ...), else
    /// `USER DEF` as the displays show it.
    #[must_use]
    pub fn display(self, autl: bool) -> &'static str {
        let all = if autl { Self::ALL_AUTL } else { Self::ALL };
        match self {
            s if s == all => "*ALL",
            s if s == Self::CHANGE => "*CHANGE",
            s if s == Self::USE => "*USE",
            s if s == Self::EXCLUDE => "*EXCLUDE",
            _ => "USER DEF",
        }
    }

    /// Each authority's name.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        NAMES
            .iter()
            .filter(|(_, b)| self.0 & b != 0)
            .map(|(n, _)| *n)
            .collect()
    }

    /// Parse a list of names and combinations (`*CHANGE *OBJMGT`);
    /// `autl` makes `*ALL` include `*AUTLMGT`.
    pub fn parse(words: &[&str], autl: bool) -> Result<Self, String> {
        let mut s = Self::EXCLUDE;
        for w in words {
            let u = w.to_ascii_uppercase();
            s = s.union(match u.as_str() {
                "*ALL" if autl => Self::ALL_AUTL,
                "*ALL" => Self::ALL,
                "*CHANGE" => Self::CHANGE,
                "*USE" => Self::USE,
                "*EXCLUDE" => Self::EXCLUDE,
                n => NAMES
                    .iter()
                    .find(|(name, _)| *name == n)
                    .map(|(_, b)| Self(*b))
                    .ok_or_else(|| format!("{w} is not an authority"))?,
            });
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations() {
        assert_eq!(Auth::parse(&["*use"], false).unwrap(), Auth::USE);
        assert_eq!(
            Auth::parse(&["*OBJOPR", "*READ", "*EXECUTE"], false)
                .unwrap()
                .display(false),
            "*USE"
        );
        assert_eq!(Auth::parse(&["*ALL"], true).unwrap(), Auth::ALL_AUTL);
        assert_eq!(Auth::ALL.display(false), "*ALL");
        assert_eq!(Auth::ALL.display(true), "USER DEF");
        assert_eq!(
            Auth::parse(&["*CHANGE", "*OBJMGT"], false)
                .unwrap()
                .display(false),
            "USER DEF"
        );
        assert_eq!(Auth::EXCLUDE.display(false), "*EXCLUDE");
        assert!(Auth::CHANGE.has(Auth::USE));
        assert_eq!(
            Auth::USE.missing(Auth::CHANGE).names(),
            vec!["*ADD", "*UPD", "*DLT"]
        );
        assert!(Auth::parse(&["*BOGUS"], false).is_err());
        assert_eq!(Auth::ALL.names().len(), 10);
    }
}
