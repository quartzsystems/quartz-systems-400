// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data model: definitions, loads, states, names and dates.

pub mod definition;
pub mod load;
pub mod state;

use std::fmt;

/// A qualified object name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualName {
    /// The object.
    pub name: String,
    /// Its library.
    pub lib: String,
}

impl QualName {
    /// A name.
    #[must_use]
    pub fn new(name: &str, lib: &str) -> Self {
        Self {
            name: name.into(),
            lib: lib.into(),
        }
    }

    /// Parse `LIB/NAME` or `NAME` (library `*LIBL`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.split_once('/') {
            Some((l, n)) => Self::new(n, l),
            None => Self::new(s, "*LIBL"),
        }
    }
}

impl fmt::Display for QualName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.lib, self.name)
    }
}

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// Four-digit year.
    pub year: u16,
    /// 1..12.
    pub month: u8,
    /// 1..31.
    pub day: u8,
}

impl Date {
    /// A date, checked.
    #[must_use]
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let d = Self { year, month, day };
        ((1..=12).contains(&month) && day >= 1 && day <= d.month_len()).then_some(d)
    }

    fn month_len(self) -> u8 {
        match self.month {
            2 if self.year.is_multiple_of(4)
                && (!self.year.is_multiple_of(100) || self.year.is_multiple_of(400)) =>
            {
                29
            }
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }

    /// Days since 1970-01-01.
    #[must_use]
    pub fn days(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = i64::from(self.month);
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after the epoch.
    #[must_use]
    pub fn from_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year: u16::try_from(year).unwrap_or(0),
            month: u8::try_from(month).unwrap_or(1),
            day: u8::try_from(day).unwrap_or(1),
        }
    }

    /// This date plus some days.
    #[must_use]
    pub fn plus(self, days: i64) -> Self {
        Self::from_days(self.days() + days)
    }

    /// Parse `YYYY-MM-DD` or `YYYYMMDD`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let digits: String = s.chars().filter(|c| *c != '-').collect();
        if digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Self::new(
            digits[..4].parse().ok()?,
            digits[4..6].parse().ok()?,
            digits[6..].parse().ok()?,
        )
    }

    /// `YYYYMMDD` as a number, the packed form of the payloads.
    #[must_use]
    pub fn ymd(self) -> u32 {
        u32::from(self.year) * 10_000 + u32::from(self.month) * 100 + u32::from(self.day)
    }

    /// The date of a `YYYYMMDD` number.
    #[must_use]
    pub fn from_ymd(n: u32) -> Option<Self> {
        Self::new(
            u16::try_from(n / 10_000).ok()?,
            u8::try_from(n / 100 % 100).ok()?,
            u8::try_from(n % 100).ok()?,
        )
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let d = Date::new(2026, 10, 30).unwrap();
        assert_eq!(Date::from_days(d.days()), d);
        assert_eq!(Date::new(1970, 1, 1).unwrap().days(), 0);
        assert_eq!(d.plus(3), Date::new(2026, 11, 2).unwrap());
        assert_eq!(Date::parse("2028-02-29"), Date::new(2028, 2, 29));
        assert!(Date::new(2027, 2, 29).is_none());
        assert_eq!(Date::from_ymd(d.ymd()), Some(d));
    }

    #[test]
    fn qualified() {
        assert_eq!(QualName::parse("QSYS/X"), QualName::new("X", "QSYS"));
        assert_eq!(QualName::parse("X").lib, "*LIBL");
    }
}
