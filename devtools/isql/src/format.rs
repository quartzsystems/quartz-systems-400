// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values as Display Data and the printed report show them, under the
//! session's date, time and decimal formats: numbers edited with
//! thousands separators and a trailing minus, no leading zero before
//! the decimal point; floats as `1.5000000000000000E+003`; dates and
//! times in the session's format (measured on pub400,
//! `tools/conformance/golden/sql/strsql/strsql-fmt*`).

/// A date format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateFmt {
    /// `yyyy-mm-dd`.
    Iso,
    /// `mm/dd/yyyy`.
    Usa,
    /// `dd.mm.yyyy`.
    Eur,
    /// `yyyy-mm-dd`.
    Jis,
    /// `mm?dd?yy`.
    Mdy,
    /// `dd?mm?yy`.
    Dmy,
    /// `yy?mm?dd`.
    Ymd,
    /// `yy?ddd`.
    Jul,
}

impl DateFmt {
    /// From `*ISO` and the like.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "*ISO" => Self::Iso,
            "*USA" => Self::Usa,
            "*EUR" => Self::Eur,
            "*JIS" => Self::Jis,
            "*MDY" => Self::Mdy,
            "*DMY" => Self::Dmy,
            "*YMD" => Self::Ymd,
            "*JUL" => Self::Jul,
            _ => return None,
        })
    }

    /// Whether the format takes the session's separator.
    #[must_use]
    pub fn has_sep(self) -> bool {
        matches!(self, Self::Mdy | Self::Dmy | Self::Ymd | Self::Jul)
    }

    /// The width of a date.
    #[must_use]
    pub fn width(self) -> usize {
        match self {
            Self::Iso | Self::Usa | Self::Eur | Self::Jis => 10,
            Self::Mdy | Self::Dmy | Self::Ymd => 8,
            Self::Jul => 6,
        }
    }
}

/// A time format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFmt {
    /// `hh?mm?ss`.
    Hms,
    /// `hh.mm.ss`.
    Iso,
    /// `hh:mm AM`.
    Usa,
    /// `hh.mm.ss`.
    Eur,
    /// `hh:mm:ss`.
    Jis,
}

impl TimeFmt {
    /// From `*HMS` and the like.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "*HMS" => Self::Hms,
            "*ISO" => Self::Iso,
            "*USA" => Self::Usa,
            "*EUR" => Self::Eur,
            "*JIS" => Self::Jis,
            _ => return None,
        })
    }

    /// Whether the format takes the session's separator.
    #[must_use]
    pub fn has_sep(self) -> bool {
        self == Self::Hms
    }
}

/// How values are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// Dates.
    pub date: DateFmt,
    /// The date separator of `MDY`, `DMY`, `YMD` and `JUL`.
    pub datsep: char,
    /// Times.
    pub time: TimeFmt,
    /// The time separator of `HMS`.
    pub timsep: char,
    /// The decimal point: `.` or `,`.
    pub point: char,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            date: DateFmt::Iso,
            datsep: '-',
            time: TimeFmt::Iso,
            timsep: '.',
            point: '.',
        }
    }
}

impl Format {
    /// The thousands separator: the other of `.` and `,`.
    #[must_use]
    pub fn thousands(&self) -> char {
        if self.point == ',' { '.' } else { ',' }
    }

    /// An ISO date (`yyyy-mm-dd`) in this format; other text as it is.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // year, month, day
    pub fn date(&self, iso: &str) -> String {
        let p: Vec<&str> = iso.split('-').collect();
        let [y, m, d] = p.as_slice() else {
            return iso.to_string();
        };
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return iso.to_string();
        }
        let s = self.datsep;
        let yy = &y[2..];
        match self.date {
            DateFmt::Iso | DateFmt::Jis => iso.to_string(),
            DateFmt::Usa => format!("{m}/{d}/{y}"),
            DateFmt::Eur => format!("{d}.{m}.{y}"),
            DateFmt::Mdy => format!("{m}{s}{d}{s}{yy}"),
            DateFmt::Dmy => format!("{d}{s}{m}{s}{yy}"),
            DateFmt::Ymd => format!("{yy}{s}{m}{s}{d}"),
            DateFmt::Jul => match ordinal(y, m, d) {
                Some(n) => format!("{yy}{s}{n:03}"),
                None => iso.to_string(),
            },
        }
    }

    /// An ISO time (`hh.mm.ss` or `hh:mm:ss`) in this format.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // hour, minute, second
    pub fn time(&self, iso: &str) -> String {
        let p: Vec<&str> = iso.split(['.', ':']).collect();
        let [h, m, s] = p.as_slice() else {
            return iso.to_string();
        };
        match self.time {
            TimeFmt::Hms => {
                let c = self.timsep;
                format!("{h}{c}{m}{c}{s}")
            }
            TimeFmt::Iso | TimeFmt::Eur => format!("{h}.{m}.{s}"),
            TimeFmt::Jis => format!("{h}:{m}:{s}"),
            TimeFmt::Usa => {
                let hour: u32 = h.parse().unwrap_or(0);
                let (h12, ap) = match hour {
                    0 => (12, "AM"),
                    1..=11 => (hour, "AM"),
                    12 => (12, "PM"),
                    _ => (hour - 12, "PM"),
                };
                format!("{h12:02}:{m} {ap}")
            }
        }
    }

    /// A decimal number (`-1234.50`) edited: thousands separators, the
    /// decimal point, a trailing minus, no zero before the point.
    #[must_use]
    pub fn number(&self, text: &str) -> String {
        let t = text.trim();
        let (neg, t) = match t.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, t.strip_prefix('+').unwrap_or(t)),
        };
        let (int, frac) = t.split_once('.').unwrap_or((t, ""));
        let int = int.trim_start_matches('0');
        let mut out = String::new();
        let n = int.len();
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (n - i) % 3 == 0 {
                out.push(self.thousands());
            }
            out.push(c);
        }
        if !frac.is_empty() {
            out.push(self.point);
            out.push_str(frac);
        } else if out.is_empty() {
            out.push('0');
        }
        let zero = int.is_empty() && frac.chars().all(|c| c == '0');
        if neg && !zero {
            out.push('-');
        }
        out
    }

    /// A float (`1500`, `1.5E3`) as `1.5000000000000000E+003`, with
    /// `digits` after the point.
    #[must_use]
    pub fn float(&self, text: &str, digits: usize) -> String {
        let Ok(x) = text.trim().parse::<f64>() else {
            return text.to_string();
        };
        let s = format!("{x:.digits$E}");
        let (m, e) = s.split_once('E').unwrap_or((&s, "0"));
        let e: i32 = e.parse().unwrap_or(0);
        let sign = if e < 0 { '-' } else { '+' };
        let m = m.replace('.', &self.point.to_string());
        format!("{m}E{sign}{:03}", e.abs())
    }
}

/// The day of the year of a date.
fn ordinal(y: &str, m: &str, d: &str) -> Option<u32> {
    let (y, m, d): (u32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let before: u32 = days
        .iter()
        .take(usize::try_from(m.checked_sub(1)?).ok()?)
        .sum();
    Some(before + d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_pub400_shows_them() {
        let ymd = Format {
            date: DateFmt::Ymd,
            datsep: '-',
            time: TimeFmt::Hms,
            timsep: ':',
            point: '.',
        };
        assert_eq!(ymd.number("-1234.50"), "1,234.50-");
        assert_eq!(ymd.number("123456789012"), "123,456,789,012");
        assert_eq!(ymd.number("0.50"), ".50");
        assert_eq!(ymd.number("-12"), "12-");
        assert_eq!(ymd.float("1500", 16), "1.5000000000000000E+003");
        assert_eq!(ymd.date("2026-01-15"), "26-01-15");
        assert_eq!(ymd.time("13.45.30"), "13:45:30");
        let usa = Format {
            date: DateFmt::Usa,
            time: TimeFmt::Usa,
            point: ',',
            ..ymd
        };
        assert_eq!(usa.number("-1234.50"), "1.234,50-");
        assert_eq!(usa.float("1500", 16), "1,5000000000000000E+003");
        assert_eq!(usa.date("2026-01-15"), "01/15/2026");
        assert_eq!(usa.time("13.45.30"), "01:45 PM");
        let jul = Format {
            date: DateFmt::Jul,
            datsep: '/',
            timsep: '.',
            ..ymd
        };
        assert_eq!(jul.date("2026-01-15"), "26/015");
        assert_eq!(jul.time("13.45.30"), "13.45.30");
    }
}
