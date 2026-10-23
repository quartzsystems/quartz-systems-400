// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Date, time and timestamp format names (`*MDY-`, `*CYMD0`, `*HMS&`)
//! and the format words the runtime library reads: the code in bits
//! 0-7, the separator in bits 8-15 (0 for none), a timestamp's
//! fractional digits in bits 16-23. The codes are `qrpgrt::dates::code`.

use crate::types::{DatFmt, TimFmt, Ty};

/// What a format describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Dates.
    Date,
    /// Times.
    Time,
    /// Timestamps.
    Stamp,
}

/// A conversion format: a code and its separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conv {
    /// The runtime's format code.
    pub code: u8,
    /// The separator; `None` for none.
    pub sep: Option<char>,
}

const DATES: &[(&str, u8, char)] = &[
    ("ISO", 1, '-'),
    ("USA", 2, '/'),
    ("EUR", 3, '.'),
    ("JIS", 4, '-'),
    ("MDY", 5, '/'),
    ("DMY", 6, '/'),
    ("YMD", 7, '/'),
    ("JUL", 8, '/'),
    ("CYMD", 9, '/'),
    ("CMDY", 10, '/'),
    ("CDMY", 11, '/'),
    ("LONGJUL", 12, '/'),
];

const TIMES: &[(&str, u8, char)] = &[("HMS", 32, ':'), ("ISO", 33, '.'), ("USA", 34, ':'), ("EUR", 35, '.'), ("JIS", 36, ':')];

/// The timestamp code.
pub const STAMP: u8 = 64;

/// Whether a format takes a separator of the programmer's choice.
fn sep_free(code: u8) -> bool {
    (5..=12).contains(&code) || code == 32
}

/// The separator a format word holds, as text.
#[must_use]
pub fn sep_text(c: Conv) -> String {
    c.sep.map(String::from).unwrap_or_default()
}

/// A format name of a class, with its separator: `*MDY-`, `*ISO0`,
/// `*HMS&` (a blank). `*JOBRUN` is the job's format, which QS/400
/// takes as `*MDY/` and `*HMS:`.
#[must_use]
pub fn parse(word: &str, class: Class) -> Option<Conv> {
    let w = word.trim_start_matches('*').to_ascii_uppercase();
    if w == "JOBRUN" {
        return Some(match class {
            Class::Time => Conv { code: 32, sep: Some(':') },
            _ => Conv { code: 5, sep: Some('/') },
        });
    }
    let (name, sep) = match w.chars().last() {
        Some(c @ ('/' | '-' | '.' | ',' | '&' | ':' | '0')) => (&w[..w.len() - 1], Some(c)),
        _ => (w.as_str(), None),
    };
    let table = match class {
        Class::Date => DATES,
        Class::Time => TIMES,
        Class::Stamp => return (name == "ISO").then_some(Conv { code: STAMP, sep: if sep == Some('0') { None } else { Some('-') } }),
    };
    let (_, code, dft) = table.iter().find(|(n, ..)| *n == name)?;
    let sep = match sep {
        None => Some(*dft),
        Some('0') if *code != 34 => None,
        Some(c) if c == *dft => Some(c),
        Some(c) if sep_free(*code) && (if *code == 32 { ":.,&" } else { "/-.,&" }).contains(c) => {
            Some(if c == '&' { ' ' } else { c })
        }
        Some(_) => return None,
    };
    Some(Conv { code: *code, sep })
}

/// The runtime code of a field's date format.
#[must_use]
pub fn date_code(f: DatFmt) -> u8 {
    DATES.iter().find(|(n, ..)| f.name()[1..] == **n).map_or(1, |x| x.1)
}

/// The runtime code of a field's time format.
#[must_use]
pub fn time_code(f: TimFmt) -> u8 {
    TIMES.iter().find(|(n, ..)| f.name()[1..] == **n).map_or(33, |x| x.1)
}

/// The format word of a conversion format.
#[must_use]
pub fn word(c: Conv, frac: u8) -> i64 {
    let sep = c.sep.map_or(0, |s| i64::from(u8::try_from(u32::from(s)).unwrap_or(b' ')));
    i64::from(c.code) | sep << 8 | i64::from(frac) << 16
}

/// The format word of a date, time or timestamp type; 0 otherwise.
#[must_use]
pub fn of_type(t: &Ty) -> i64 {
    let none = |s: char| (s != '\0').then_some(s);
    match t {
        Ty::Date(f, s) => word(Conv { code: date_code(*f), sep: none(*s) }, 0),
        Ty::Time(f, s) => word(Conv { code: time_code(*f), sep: none(*s) }, 0),
        Ty::Timestamp(p) => word(Conv { code: STAMP, sep: Some('-') }, *p),
        _ => 0,
    }
}

/// A field format keyword's argument (`DATFMT(*MDY-)`): the format and
/// separator a date field may have, `'\0'` for none.
#[must_use]
pub fn field_date(word: &str) -> Option<(DatFmt, char)> {
    let c = parse(word, Class::Date)?;
    let f = DatFmt::parse(DATES.iter().find(|x| x.1 == c.code)?.0)?;
    Some((f, c.sep.unwrap_or('\0')))
}

/// A field's `TIMFMT` argument.
#[must_use]
pub fn field_time(word: &str) -> Option<(TimFmt, char)> {
    let c = parse(word, Class::Time)?;
    let f = TimFmt::parse(TIMES.iter().find(|x| x.1 == c.code)?.0)?;
    Some((f, c.sep.unwrap_or('\0')))
}

/// The characters of a civil date in a field's format.
#[must_use]
pub fn render_date(f: DatFmt, sep: char, y: u32, m: u32, d: u32) -> String {
    let s = if sep == '\0' { String::new() } else { sep.to_string() };
    let yy = y % 100;
    match f {
        DatFmt::Iso | DatFmt::Jis => format!("{y:04}{s}{m:02}{s}{d:02}"),
        DatFmt::Usa => format!("{m:02}{s}{d:02}{s}{y:04}"),
        DatFmt::Eur => format!("{d:02}{s}{m:02}{s}{y:04}"),
        DatFmt::Mdy => format!("{m:02}{s}{d:02}{s}{yy:02}"),
        DatFmt::Dmy => format!("{d:02}{s}{m:02}{s}{yy:02}"),
        DatFmt::Ymd => format!("{yy:02}{s}{m:02}{s}{d:02}"),
        DatFmt::Jul => {
            let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
            let before: u32 = (1..m)
                .map(|k| match k {
                    2 if leap => 29,
                    2 => 28,
                    4 | 6 | 9 | 11 => 30,
                    _ => 31,
                })
                .sum();
            format!("{yy:02}{s}{:03}", before + d)
        }
    }
}

/// The characters of a time in a field's format.
#[must_use]
pub fn render_time(f: TimFmt, sep: char, h: u32, m: u32, s: u32) -> String {
    let c = if sep == '\0' { String::new() } else { sep.to_string() };
    if f == TimFmt::Usa {
        let (h12, ap) = match h {
            0 => (0, "AM"),
            24 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        return format!("{h12:02}{c}{m:02} {ap}");
    }
    format!("{h:02}{c}{m:02}{c}{s:02}")
}

/// Split text into fields of the given widths, with `sep` between them
/// (none when `'\0'`).
fn fields(s: &str, widths: &[usize], sep: char) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    let mut rest = s;
    for (i, w) in widths.iter().enumerate() {
        if i > 0 && sep != '\0' {
            rest = rest.strip_prefix(sep)?;
        }
        let f = rest.get(..*w)?;
        if !f.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out.push(f.parse().ok()?);
        rest = &rest[*w..];
    }
    rest.is_empty().then_some(out)
}

/// A date literal in a field format: year, month, day.
#[must_use]
pub fn parse_date(f: DatFmt, sep: char, s: &str) -> Option<(u32, u32, u32)> {
    let two = |y: u32| if y >= 40 { 1900 + y } else { 2000 + y };
    let (y, m, d) = match f {
        DatFmt::Iso | DatFmt::Jis => {
            let v = fields(s, &[4, 2, 2], sep)?;
            (v[0], v[1], v[2])
        }
        DatFmt::Usa => {
            let v = fields(s, &[2, 2, 4], sep)?;
            (v[2], v[0], v[1])
        }
        DatFmt::Eur => {
            let v = fields(s, &[2, 2, 4], sep)?;
            (v[2], v[1], v[0])
        }
        DatFmt::Mdy => {
            let v = fields(s, &[2, 2, 2], sep)?;
            (two(v[2]), v[0], v[1])
        }
        DatFmt::Dmy => {
            let v = fields(s, &[2, 2, 2], sep)?;
            (two(v[2]), v[1], v[0])
        }
        DatFmt::Ymd => {
            let v = fields(s, &[2, 2, 2], sep)?;
            (two(v[0]), v[1], v[2])
        }
        DatFmt::Jul => {
            let v = fields(s, &[2, 3], sep)?;
            let y = two(v[0]);
            let mut n = v[1];
            let mut m = 1;
            while m <= 12 && n > month_len(y, m) {
                n -= month_len(y, m);
                m += 1;
            }
            (y, m, n)
        }
    };
    ((1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= month_len(y, m)).then_some((y, m, d))
}

fn month_len(y: u32, m: u32) -> u32 {
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A time literal in a field format: hour, minute, second.
#[must_use]
pub fn parse_time(f: TimFmt, sep: char, s: &str) -> Option<(u32, u32, u32)> {
    if f == TimFmt::Usa {
        let (hm, ap) = s.split_at_checked(s.len().checked_sub(3)?)?;
        let v = fields(hm, &[2, 2], sep)?;
        let h = match ap {
            " AM" if v[0] <= 12 => v[0] % 12,
            " PM" if v[0] <= 12 => v[0] % 12 + 12,
            _ => return None,
        };
        return (v[1] < 60).then_some((h, v[1], 0));
    }
    let v = fields(s, &[2, 2, 2], sep)?;
    (v[0] < 24 && v[1] < 60 && v[2] < 60 || v[0] == 24 && v[1] == 0 && v[2] == 0).then_some((v[0], v[1], v[2]))
}

/// The characters a conversion format takes, separators included.
#[must_use]
pub fn conv_len(c: Conv, class: Class) -> usize {
    let (n, seps) = match (class, c.code) {
        (Class::Stamp, _) => return if c.sep.is_some() { 26 } else { 20 },
        (Class::Time, _) => (6, 2),
        (_, 1..=4) => (8, 2),
        (_, 5..=7) => (6, 2),
        (_, 8) => (5, 1),
        (_, 9..=11) => (7, 2),
        _ => (7, 1),
    };
    if c.sep.is_some() { n + seps } else { n }
}

/// The lowest and highest years a date format holds.
#[must_use]
pub fn years(f: DatFmt) -> (u32, u32) {
    match f {
        DatFmt::Mdy | DatFmt::Dmy | DatFmt::Ymd | DatFmt::Jul => (1940, 2039),
        _ => (1, 9999),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_with_separators() {
        assert_eq!(parse("*MDY-", Class::Date), Some(Conv { code: 5, sep: Some('-') }));
        assert_eq!(parse("*ISO0", Class::Date), Some(Conv { code: 1, sep: None }));
        assert_eq!(parse("*JUL&", Class::Date), Some(Conv { code: 8, sep: Some(' ') }));
        assert_eq!(parse("*HMS,", Class::Time), Some(Conv { code: 32, sep: Some(',') }));
        assert_eq!(parse("*ISO/", Class::Date), None);
        assert_eq!(render_date(DatFmt::Jul, '/', 2026, 3, 5), "26/064");
        assert_eq!(render_time(TimFmt::Usa, ':', 14, 30, 5), "02:30 PM");
    }
}
