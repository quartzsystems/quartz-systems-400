// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dates and times as text: parsing any DDS format, rendering one.
//! Two-digit years 40 to 99 are 1940 to 1999, 00 to 39 are 2000 to 2039.

use dds::{DateFmt, TimeFmt};

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    /// Year, four digits.
    pub year: u16,
    /// Month 1 to 12.
    pub month: u8,
    /// Day of the month.
    pub day: u8,
}

/// A time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    /// Hour 0 to 23.
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// Second.
    pub second: u8,
}

fn digits(text: &str) -> Vec<u8> {
    text.bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect()
}

fn num(ds: &[u8]) -> u32 {
    ds.iter().fold(0u32, |acc, d| acc * 10 + u32::from(*d))
}

fn century(yy: u32) -> u16 {
    u16::try_from(if yy >= 40 { 1900 + yy } else { 2000 + yy }).expect("small")
}

/// Whether `year` is a leap year.
#[must_use]
pub fn is_leap(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Days in a month.
#[must_use]
pub fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    /// The civil date of a Unix time (UTC).
    #[must_use]
    pub fn from_unix(secs: u64) -> Self {
        // Days since 1970-01-01 to a proleptic Gregorian date.
        let z = i64::try_from(secs / 86_400).expect("fits") + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).expect("day");
        let month = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).expect("month");
        let year = yoe + era * 400 + i64::from(month <= 2);
        Self {
            year: u16::try_from(year).expect("year"),
            month,
            day,
        }
    }

    /// A date, checked.
    #[must_use]
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        (1..=12)
            .contains(&month)
            .then(|| days_in_month(year, month))
            .filter(|&n| day >= 1 && day <= n)
            .map(|_| Self { year, month, day })
    }

    /// From a year and a day of the year.
    #[must_use]
    pub fn from_day_of_year(year: u16, mut day: u16) -> Option<Self> {
        for month in 1..=12u8 {
            let n = u16::from(days_in_month(year, month));
            if day <= n {
                return Self::new(year, month, u8::try_from(day).ok()?);
            }
            day -= n;
        }
        None
    }

    /// Day of the year, 1 to 366.
    #[must_use]
    pub fn day_of_year(self) -> u16 {
        (1..self.month)
            .map(|m| u16::from(days_in_month(self.year, m)))
            .sum::<u16>()
            + u16::from(self.day)
    }

    /// Parse text in `fmt`, separators ignored.
    #[must_use]
    pub fn parse(text: &str, fmt: DateFmt) -> Option<Self> {
        let d = digits(text);
        let (y, m, dd) = match (fmt, d.len()) {
            (DateFmt::Iso | DateFmt::Jis, 8) => (num(&d[..4]), num(&d[4..6]), num(&d[6..8])),
            (DateFmt::Usa, 8) => (num(&d[4..8]), num(&d[..2]), num(&d[2..4])),
            (DateFmt::Eur, 8) => (num(&d[4..8]), num(&d[2..4]), num(&d[..2])),
            (DateFmt::Mdy, 6) => (
                u32::from(century(num(&d[4..6]))),
                num(&d[..2]),
                num(&d[2..4]),
            ),
            (DateFmt::Dmy, 6) => (
                u32::from(century(num(&d[4..6]))),
                num(&d[2..4]),
                num(&d[..2]),
            ),
            (DateFmt::Ymd, 6) => (
                u32::from(century(num(&d[..2]))),
                num(&d[2..4]),
                num(&d[4..6]),
            ),
            (DateFmt::Jul, 5) => {
                let year = century(num(&d[..2]));
                return Self::from_day_of_year(year, u16::try_from(num(&d[2..5])).ok()?);
            }
            _ => return None,
        };
        Self::new(
            u16::try_from(y).ok()?,
            u8::try_from(m).ok()?,
            u8::try_from(dd).ok()?,
        )
    }

    /// Render in `fmt` with `sep` (the format's own when it has one,
    /// `/` when none is given).
    #[must_use]
    pub fn render(self, fmt: DateFmt, sep: Option<char>) -> String {
        let s = fmt.fixed_separator().or(sep).unwrap_or('/');
        let yy = self.year % 100;
        let (a, b, c) = match fmt {
            DateFmt::Iso | DateFmt::Jis => {
                return format!("{:04}{s}{:02}{s}{:02}", self.year, self.month, self.day);
            }
            DateFmt::Usa => {
                return format!("{:02}{s}{:02}{s}{:04}", self.month, self.day, self.year);
            }
            DateFmt::Eur => {
                return format!("{:02}{s}{:02}{s}{:04}", self.day, self.month, self.year);
            }
            DateFmt::Mdy => (u16::from(self.month), u16::from(self.day), yy),
            DateFmt::Dmy => (u16::from(self.day), u16::from(self.month), yy),
            DateFmt::Ymd => (yy, u16::from(self.month), u16::from(self.day)),
            DateFmt::Jul => return format!("{yy:02}{s}{:03}", self.day_of_year()),
        };
        format!("{a:02}{s}{b:02}{s}{c:02}")
    }
}

impl Time {
    /// The time of day of a Unix time (UTC).
    #[must_use]
    pub fn from_unix(secs: u64) -> Self {
        let s = secs % 86_400;
        Self {
            hour: u8::try_from(s / 3600).expect("hour"),
            minute: u8::try_from(s % 3600 / 60).expect("minute"),
            second: u8::try_from(s % 60).expect("second"),
        }
    }

    /// A time, checked.
    #[must_use]
    pub fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        (hour < 24 && minute < 60 && second < 60).then_some(Self {
            hour,
            minute,
            second,
        })
    }

    /// Parse text in `fmt`; `*USA` takes `hh:mm AM`, the others
    /// `hh mm ss` with any separators.
    #[must_use]
    pub fn parse(text: &str, fmt: TimeFmt) -> Option<Self> {
        let d = digits(text);
        let upper = text.to_ascii_uppercase();
        if fmt == TimeFmt::Usa && d.len() == 4 {
            let mut h = u8::try_from(num(&d[..2])).ok()?;
            let pm = upper.contains("PM");
            let am = upper.contains("AM");
            if !(am || pm) || h == 0 || h > 12 {
                return None;
            }
            h = match (h, pm) {
                (12, false) => 0,
                (12, true) => 12,
                (h, true) => h + 12,
                (h, false) => h,
            };
            return Self::new(h, u8::try_from(num(&d[2..4])).ok()?, 0);
        }
        if d.len() != 6 {
            return None;
        }
        Self::new(
            u8::try_from(num(&d[..2])).ok()?,
            u8::try_from(num(&d[2..4])).ok()?,
            u8::try_from(num(&d[4..6])).ok()?,
        )
    }

    /// The text a field of `fmt` stores. `*USA` is `hh:mm:ss`: the
    /// machine keeps a `*USA` time's seconds, which `hh:mm AM` has no
    /// room for; every other format stores its rendering.
    #[must_use]
    pub fn stored(self, fmt: TimeFmt, sep: Option<char>) -> String {
        if fmt == TimeFmt::Usa {
            return format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second);
        }
        self.render(fmt, sep)
    }

    /// Render in `fmt` with `sep` (`:` when the format has none and
    /// none is given).
    #[must_use]
    pub fn render(self, fmt: TimeFmt, sep: Option<char>) -> String {
        let s = fmt.fixed_separator().or(sep).unwrap_or(':');
        if fmt == TimeFmt::Usa {
            let (h, half) = match self.hour {
                0 => (12, "AM"),
                12 => (12, "PM"),
                h if h > 12 => (h - 12, "PM"),
                h => (h, "AM"),
            };
            return format!("{h:02}{s}{:02} {half}", self.minute);
        }
        format!("{:02}{s}{:02}{s}{:02}", self.hour, self.minute, self.second)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        let d = Date::parse("12/31/99", DateFmt::Mdy).unwrap();
        assert_eq!(d, Date::new(1999, 12, 31).unwrap());
        assert_eq!(d.render(DateFmt::Iso, None), "1999-12-31");
        assert_eq!(d.render(DateFmt::Usa, None), "12/31/1999");
        assert_eq!(d.render(DateFmt::Eur, None), "31.12.1999");
        assert_eq!(d.render(DateFmt::Dmy, Some('-')), "31-12-99");
        assert_eq!(d.render(DateFmt::Jul, None), "99/365");
        assert_eq!(
            Date::parse("26/060", DateFmt::Jul).unwrap(),
            Date::new(2026, 3, 1).unwrap()
        );
        assert_eq!(
            Date::parse("24/060", DateFmt::Jul).unwrap(),
            Date::new(2024, 2, 29).unwrap()
        );
        assert!(Date::parse("2026-02-29", DateFmt::Iso).is_none());
        assert!(Date::parse("13/01/2026", DateFmt::Usa).is_none());
        assert_eq!(
            Date::parse("2026-03-01", DateFmt::Iso)
                .unwrap()
                .day_of_year(),
            60
        );
    }

    #[test]
    fn unix_times() {
        assert_eq!(Date::from_unix(0), Date::new(1970, 1, 1).unwrap());
        assert_eq!(
            Date::from_unix(1_767_225_600),
            Date::new(2026, 1, 1).unwrap()
        );
        assert_eq!(
            Date::from_unix(951_782_400),
            Date::new(2000, 2, 29).unwrap()
        );
        assert_eq!(
            Time::from_unix(1_767_225_600 + 3661),
            Time::new(1, 1, 1).unwrap()
        );
    }

    #[test]
    fn times_round_trip() {
        let t = Time::parse("13.05.09", TimeFmt::Iso).unwrap();
        assert_eq!(t.render(TimeFmt::Hms, None), "13:05:09");
        assert_eq!(t.render(TimeFmt::Usa, None), "01:05 PM");
        assert_eq!(
            Time::parse("12:30 AM", TimeFmt::Usa).unwrap(),
            Time::new(0, 30, 0).unwrap()
        );
        assert_eq!(
            Time::parse("12:30 PM", TimeFmt::Usa).unwrap(),
            Time::new(12, 30, 0).unwrap()
        );
        assert_eq!(
            Time::new(0, 0, 0).unwrap().render(TimeFmt::Usa, None),
            "12:00 AM"
        );
        assert!(Time::parse("24:00:00", TimeFmt::Hms).is_none());
        // A *USA field stores its seconds and reads them back.
        let t = Time::new(13, 5, 9).unwrap();
        assert_eq!(t.stored(TimeFmt::Usa, None), "13:05:09");
        assert_eq!(Time::parse("13:05:09", TimeFmt::Usa), Some(t));
        assert_eq!(t.stored(TimeFmt::Hms, Some('.')), "13.05.09");
    }
}
