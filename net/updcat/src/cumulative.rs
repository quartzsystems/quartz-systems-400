// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A cumulative package's name, `Cyyddd110`: the year (two digits,
//! 20yy) and the day of that year it was made, as IBM names them.

use std::fmt;
use std::str::FromStr;

/// A cumulative package's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cumulative {
    /// Two-digit year.
    pub year: u8,
    /// Day of the year, 1 to 366.
    pub day: u16,
}

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// The year, four digits.
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Not a cumulative package's name.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("not a cumulative package name: {0}")]
pub struct NotCumulative(pub String);

impl Cumulative {
    /// The date its name carries; `None` for day 366 of a year without it.
    #[must_use]
    pub fn date(self) -> Option<Date> {
        let year = 2000 + u16::from(self.year);
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let months = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut left = self.day;
        for (m, len) in (1u8..).zip(months) {
            if left <= len {
                return Some(Date {
                    year,
                    month: m,
                    day: u8::try_from(left).ok()?,
                });
            }
            left -= len;
        }
        None
    }
}

impl fmt::Display for Cumulative {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C{:02}{:03}110", self.year, self.day)
    }
}

impl FromStr for Cumulative {
    type Err = NotCumulative;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || NotCumulative(s.to_string());
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
    fn names_and_dates() {
        let c: Cumulative = "C26300110".parse().unwrap();
        assert_eq!(c, Cumulative { year: 26, day: 300 });
        assert_eq!(c.to_string(), "C26300110");
        assert_eq!(c.date().unwrap().to_string(), "2026-10-27");
        let d = |s: &str| s.parse::<Cumulative>().unwrap().date().map(|d| d.to_string());
        assert_eq!(d("C24060110").as_deref(), Some("2024-02-29"));
        assert_eq!(d("C25060110").as_deref(), Some("2025-03-01"));
        assert_eq!(d("C24366110").as_deref(), Some("2024-12-31"));
        assert_eq!(d("C25366110"), None);
        assert_eq!(d("C25001110").as_deref(), Some("2025-01-01"));
        for bad in ["C2630011", "X26300110", "C26000110", "C26367110", "C2a300110", "QS99001"] {
            assert!(bad.parse::<Cumulative>().is_err(), "{bad}");
        }
    }
}
