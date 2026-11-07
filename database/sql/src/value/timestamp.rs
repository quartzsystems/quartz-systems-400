// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Timestamps to the picosecond: `TIMESTAMP(p)` with `p` 0 to 12.

use std::cmp::Ordering;
use std::fmt;

use db::record::datetime::{Date, Time};
use dds::{DateFmt, TimeFmt};

use super::{cmp_date, cmp_time};

/// Picoseconds in a second.
pub const PICOS: u64 = 1_000_000_000_000;

/// A timestamp: a date, a time and the picoseconds past the second.
/// `precision` is the fractional digits it shows (0 to 12); the digits
/// past it are kept, so a clock's value keeps what the clock gave.
#[derive(Clone, Copy, Debug)]
pub struct Timestamp {
    /// Date.
    pub date: Date,
    /// Time.
    pub time: Time,
    /// Picoseconds, below 10^12.
    pub picos: u64,
    /// Fractional digits shown.
    pub precision: u8,
}

impl Timestamp {
    /// A timestamp of precision 6.
    #[must_use]
    pub fn new(date: Date, time: Time, picos: u64) -> Self {
        Self {
            date,
            time,
            picos,
            precision: 6,
        }
    }

    /// Midnight of a date.
    #[must_use]
    pub fn midnight(date: Date) -> Self {
        Self::new(date, Time::new(0, 0, 0).expect("midnight"), 0)
    }

    /// Parse `yyyy-mm-dd hh:mm:ss[.f]` or the `-`/`.` IBM spelling, up to
    /// 12 fractional digits. The precision is the digits given, at least
    /// 6.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.len() < 19 || !t.is_char_boundary(19) {
            return None;
        }
        let date = Date::parse(&t[..10], DateFmt::Iso)?;
        let time = Time::parse(&t[11..19], TimeFmt::Hms)
            .or_else(|| Time::parse(&t[11..19], TimeFmt::Iso))?;
        let (picos, digits) = match t.get(19..) {
            None | Some("") => (0, 0),
            Some(f) => {
                let f = f.strip_prefix(['.', ',', ':'])?;
                if f.len() > 12 || !f.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                let n: u64 = if f.is_empty() { 0 } else { f.parse().ok()? };
                (
                    n * 10u64.pow(12 - u32::try_from(f.len()).expect("small")),
                    f.len(),
                )
            }
        };
        Some(Self {
            date,
            time,
            picos,
            precision: u8::try_from(digits.max(6)).expect("small"),
        })
    }

    /// As [`Self::parse`], the precision exactly the digits given.
    #[must_use]
    pub fn parse_exact(text: &str) -> Option<Self> {
        let digits = text.trim().get(20..).map_or(0, str::len);
        Self::parse(text).map(|t| Self {
            precision: u8::try_from(digits.min(12)).expect("small"),
            ..t
        })
    }

    /// Microseconds past the second.
    #[must_use]
    pub fn micros(&self) -> u32 {
        u32::try_from(self.picos / 1_000_000).expect("below a million")
    }

    /// As `TIMESTAMP(p)`: digits past `p` dropped.
    #[must_use]
    pub fn with_precision(self, p: u8) -> Self {
        let unit = 10u64.pow(12 - u32::from(p.min(12)));
        Self {
            picos: self.picos / unit * unit,
            precision: p.min(12),
            ..self
        }
    }

    /// The fraction as shown: `.` and `precision` digits, or nothing.
    fn fraction(&self) -> String {
        if self.precision == 0 {
            return String::new();
        }
        let digits = format!("{:012}", self.picos);
        format!(".{}", &digits[..usize::from(self.precision)])
    }

    /// `yyyy-mm-dd-hh.mm.ss[.f]`, as a `Z` field holds it.
    #[must_use]
    pub fn render_field(&self) -> String {
        format!(
            "{}-{}{}",
            self.date.render(DateFmt::Iso, None),
            self.time.render(TimeFmt::Iso, None),
            self.fraction()
        )
    }

    /// Text that is equal for equal timestamps whatever their precision.
    #[must_use]
    pub fn key(&self) -> String {
        Self {
            precision: 12,
            ..*self
        }
        .render_field()
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}{}",
            self.date.render(DateFmt::Iso, None),
            self.time.render(TimeFmt::Hms, None),
            self.fraction()
        )
    }
}

impl PartialEq for Timestamp {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Timestamp {}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        cmp_date(&self.date, &other.date)
            .then_with(|| cmp_time(&self.time, &other.time))
            .then(self.picos.cmp(&other.picos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precisions() {
        let ts = Timestamp::parse("2026-01-02-10.20.30.000123").unwrap();
        assert_eq!(ts.to_string(), "2026-01-02 10:20:30.000123");
        assert_eq!(ts.render_field(), "2026-01-02-10.20.30.000123");
        assert_eq!(ts.micros(), 123);
        let fine = Timestamp::parse("2026-01-02 10:20:30.123456789012").unwrap();
        assert_eq!((fine.precision, fine.picos), (12, 123_456_789_012));
        assert_eq!(fine.render_field(), "2026-01-02-10.20.30.123456789012");
        assert_eq!(
            fine.with_precision(3).to_string(),
            "2026-01-02 10:20:30.123"
        );
        assert_eq!(fine.with_precision(0).render_field(), "2026-01-02-10.20.30");
        assert!(fine > ts);
        assert_eq!(
            Timestamp::parse("2026-01-02 10:20:30").unwrap().precision,
            6
        );
        assert_eq!(
            Timestamp::parse("2026-01-02-10.20.30")
                .unwrap()
                .with_precision(0)
                .precision,
            0
        );
        assert!(Timestamp::parse("2026-01-02 10:20:30.1234567890123").is_none());
        assert_eq!(
            fine.with_precision(9),
            Timestamp::parse("2026-01-02 10:20:30.123456789").unwrap()
        );
        assert_eq!(
            fine.with_precision(9).key(),
            Timestamp::parse("2026-01-02 10:20:30.123456789000")
                .unwrap()
                .key()
        );
    }
}
