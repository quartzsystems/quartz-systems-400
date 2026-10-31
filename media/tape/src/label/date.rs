// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Label dates: `cyyddd`, where `c` is blank for 19xx, `0` for 20xx,
//! `1` for 21xx and so on, `yy` the year in the century and `ddd` the
//! day of the year.

/// A calendar day as year and day of the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    /// Year, 1900 to 2899.
    pub year: u16,
    /// Day of the year, 1 to 366.
    pub day: u16,
}

fn leap(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

impl Date {
    /// From year, month and day. `None` when the date does not exist.
    #[must_use]
    pub fn from_ymd(year: u16, month: u8, day: u8) -> Option<Self> {
        const DAYS: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        if !(1..=12).contains(&month) || day == 0 || !(1900..=2899).contains(&year) {
            return None;
        }
        let m = usize::from(month - 1);
        let feb = u16::from(leap(year));
        let len = DAYS[m] + if m == 1 { feb } else { 0 };
        if u16::from(day) > len {
            return None;
        }
        let before: u16 = DAYS[..m].iter().sum::<u16>() + if m > 1 { feb } else { 0 };
        Some(Self {
            year,
            day: before + u16::from(day),
        })
    }

    /// The six label characters.
    #[must_use]
    pub fn to_label(self) -> String {
        let c = match self.year / 100 {
            19 => ' ',
            n => char::from(b'0' + u8::try_from(n - 20).unwrap_or(9)),
        };
        format!("{c}{:02}{:03}", self.year % 100, self.day)
    }

    /// From six label characters. `None` for zeros or blanks.
    #[must_use]
    pub fn from_label(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 6 || !b[1..].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let century = match b[0] {
            b' ' => 1900,
            c @ b'0'..=b'9' => 2000 + 100 * u16::from(c - b'0'),
            _ => return None,
        };
        let yy: u16 = s[1..3].parse().ok()?;
        let day: u16 = s[3..].parse().ok()?;
        if day == 0 || day > 366 {
            return None;
        }
        Some(Self {
            year: century + yy,
            day,
        })
    }
}

/// A file's expiration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Expiry {
    /// No expiration date: the file may be overwritten (EXPDATE(*NONE)).
    #[default]
    None,
    /// Protected until this day.
    Date(Date),
    /// Never expires (EXPDATE(*PERM)).
    Perm,
}

/// The label form of *PERM: year 99, day 365, blank century.
pub const PERM_LABEL: &str = " 99365";

impl Expiry {
    /// The six label characters.
    #[must_use]
    pub fn to_label(self) -> String {
        match self {
            Self::None => " 00000".to_string(),
            Self::Date(d) => d.to_label(),
            Self::Perm => PERM_LABEL.to_string(),
        }
    }

    /// From six label characters; 99365, 99366 and 99999 in any century
    /// are *PERM.
    #[must_use]
    pub fn from_label(s: &str) -> Self {
        if s.len() == 6 && matches!(&s[1..], "99365" | "99366" | "99999") {
            return Self::Perm;
        }
        Date::from_label(s).map_or(Self::None, Self::Date)
    }

    /// Whether the file is still protected on `today`.
    #[must_use]
    pub fn active(self, today: Date) -> bool {
        match self {
            Self::None => false,
            Self::Date(d) => d > today,
            Self::Perm => true,
        }
    }
}
