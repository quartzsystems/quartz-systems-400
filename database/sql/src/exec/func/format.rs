// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The format strings of `VARCHAR_FORMAT` and `TIMESTAMP_FORMAT`: date
//! and time elements (`YYYY`, `MM`, `DD`, `HH24`, `MI`, `SS`, `FFn`,
//! `Month`, `Day`, `IW`, ...) both ways, and number formats (`9`, `0`,
//! `,`/`G`, `.`/`D`, `S`, `MI`, `PR`, `$`/`L`) for numbers to text.

#![allow(clippy::many_single_char_names)]

use db::record::datetime::{Date, Time, days_in_month};
use qdecimal::Rounding;

use super::super::SqlContext;
use super::super::eval::{date_from_days, days_of};
use super::day_of_week;
use crate::error::{Result, SqlError};
use crate::value::{PICOS, Timestamp, Value};

const MONTHS: [&str; 12] = [
    "JANUARY",
    "FEBRUARY",
    "MARCH",
    "APRIL",
    "MAY",
    "JUNE",
    "JULY",
    "AUGUST",
    "SEPTEMBER",
    "OCTOBER",
    "NOVEMBER",
    "DECEMBER",
];
const DAYS: [&str; 7] = [
    "SUNDAY",
    "MONDAY",
    "TUESDAY",
    "WEDNESDAY",
    "THURSDAY",
    "FRIDAY",
    "SATURDAY",
];

/// Elements, longest first so the longest matches.
const ELEMENTS: &[&str] = &[
    "NNNNNN", "SSSSS", "MONTH", "SYYYY", "A.M.", "P.M.", "HH24", "HH12", "IYYY", "RRRR", "YYYY",
    "FF10", "FF11", "FF12", "DDD", "IYY", "MON", "DAY", "YYY", "FF1", "FF2", "FF3", "FF4", "FF5",
    "FF6", "FF7", "FF8", "FF9", "YY", "RR", "IY", "CC", "MM", "RM", "WW", "IW", "DD", "DY", "HH",
    "MI", "SS", "AM", "PM", "FF", "Q", "W", "D", "J", "I",
];

/// A piece of a format: an element (upper case, with the case it was
/// written in) or literal text.
enum Piece {
    Elem(&'static str, String),
    Lit(char),
}

fn pieces(fmt: &str) -> Vec<Piece> {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    'next: while i < chars.len() {
        for e in ELEMENTS {
            let n = e.chars().count();
            if i + n <= chars.len() {
                let written: String = chars[i..i + n].iter().collect();
                if written.to_ascii_uppercase() == *e {
                    out.push(Piece::Elem(e, written));
                    i += n;
                    continue 'next;
                }
            }
        }
        out.push(Piece::Lit(chars[i]));
        i += 1;
    }
    out
}

/// A name in the case its element was written in: `MONTH`, `Month`,
/// `month`.
fn cased(name: &str, written: &str) -> String {
    let mut w = written.chars();
    match (w.next(), w.next()) {
        (Some(a), Some(b)) if a.is_ascii_uppercase() && b.is_ascii_lowercase() => {
            let lower = name.to_ascii_lowercase();
            let mut out = String::new();
            for (i, c) in lower.chars().enumerate() {
                out.push(if i == 0 { c.to_ascii_uppercase() } else { c });
            }
            out
        }
        (Some(a), _) if a.is_ascii_lowercase() => name.to_ascii_lowercase(),
        _ => name.to_string(),
    }
}

fn not_valid(what: &str) -> SqlError {
    SqlError::Conversion {
        what: format!("{what} does not match the format string"),
    }
}

/// A timestamp as `VARCHAR_FORMAT` shows it.
pub(crate) fn timestamp_text(t: &Timestamp, fmt: &str) -> Result<String> {
    let d = t.date;
    let mut out = String::new();
    let hour12 = match t.time.hour % 12 {
        0 => 12,
        h => h,
    };
    for p in pieces(fmt) {
        match p {
            Piece::Lit(c) => out.push(c),
            Piece::Elem(e, w) => {
                let s = match e {
                    "YYYY" | "SYYYY" | "RRRR" => format!("{:04}", d.year),
                    "YYY" => format!("{:03}", d.year % 1000),
                    "YY" | "RR" => format!("{:02}", d.year % 100),
                    "CC" => format!("{:02}", u32::from(d.year).div_ceil(100)),
                    "IYYY" => format!("{:04}", super::datetime::iso_week(d).0),
                    "IYY" => format!("{:03}", super::datetime::iso_week(d).0 % 1000),
                    "IY" => format!("{:02}", super::datetime::iso_week(d).0 % 100),
                    "I" => format!("{}", super::datetime::iso_week(d).0 % 10),
                    "Q" => format!("{}", (d.month - 1) / 3 + 1),
                    "MM" => format!("{:02}", d.month),
                    "RM" => [
                        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
                    ][usize::from(d.month - 1)]
                    .to_string(),
                    "MONTH" => cased(MONTHS[usize::from(d.month - 1)], &w),
                    "MON" => cased(&MONTHS[usize::from(d.month - 1)][..3], &w),
                    "DAY" => cased(DAYS[usize::try_from(day_of_week(d) - 1).expect("0..7")], &w),
                    "DY" => cased(
                        &DAYS[usize::try_from(day_of_week(d) - 1).expect("0..7")][..3],
                        &w,
                    ),
                    "D" => format!("{}", day_of_week(d)),
                    "DD" => format!("{:02}", d.day),
                    "DDD" => format!("{:03}", d.day_of_year()),
                    "J" => format!("{}", days_of(d) + 1_721_425),
                    "W" => format!("{}", (d.day - 1) / 7 + 1),
                    "WW" => format!("{:02}", (d.day_of_year() - 1) / 7 + 1),
                    "IW" => format!("{:02}", super::datetime::iso_week(d).1),
                    "HH" | "HH12" => format!("{hour12:02}"),
                    "HH24" => format!("{:02}", t.time.hour),
                    "MI" => format!("{:02}", t.time.minute),
                    "SS" => format!("{:02}", t.time.second),
                    "SSSSS" => format!(
                        "{:05}",
                        u32::from(t.time.hour) * 3600
                            + u32::from(t.time.minute) * 60
                            + u32::from(t.time.second)
                    ),
                    "NNNNNN" => format!("{:06}", t.micros()),
                    "AM" | "PM" => (if t.time.hour < 12 { "AM" } else { "PM" }).to_string(),
                    "A.M." | "P.M." => (if t.time.hour < 12 { "A.M." } else { "P.M." }).to_string(),
                    ff => {
                        let n: usize = ff.trim_start_matches("FF").parse().unwrap_or(6);
                        format!("{:012}", t.picos)[..n.min(12)].to_string()
                    }
                };
                out.push_str(&s);
            }
        }
    }
    if out.chars().count() > 255 {
        return Err(SqlError::TooLong {
            what: "the VARCHAR_FORMAT result".into(),
        });
    }
    Ok(out)
}

/// Up to `max` digits at `pos`; the value and the digits read.
fn digits(s: &[char], pos: &mut usize, max: usize) -> Option<u32> {
    let start = *pos;
    while *pos < s.len() && *pos - start < max && s[*pos].is_ascii_digit() {
        *pos += 1;
    }
    if *pos == start {
        return None;
    }
    s[start..*pos].iter().collect::<String>().parse().ok()
}

#[allow(clippy::too_many_lines)]
/// Text read by `TIMESTAMP_FORMAT`'s format string. Elements not given
/// take the defaults: the current year, January, the 1st, midnight.
pub(crate) fn parse_timestamp(text: &str, fmt: &str, ctx: &SqlContext) -> Result<Timestamp> {
    let s: Vec<char> = text.trim().chars().collect();
    let mut pos = 0;
    let now = Date::from_unix(ctx.clock());
    let (mut year, mut month, mut day) = (u32::from(now.year), 1u32, 1u32);
    let mut doy: Option<u32> = None;
    let (mut hour, mut minute, mut second) = (0u32, 0u32, 0u32);
    let mut picos = 0u64;
    let mut pm: Option<bool> = None;
    let bad = || not_valid(&format!("'{text}'"));
    for p in pieces(fmt) {
        match p {
            Piece::Lit(c) => {
                // A separator matches any one separator, or none.
                if pos < s.len() && !s[pos].is_ascii_alphanumeric() {
                    pos += 1;
                } else if c.is_ascii_alphanumeric() {
                    return Err(bad());
                }
            }
            Piece::Elem(e, _) => match e {
                "YYYY" | "SYYYY" | "RRRR" | "IYYY" => {
                    year = digits(&s, &mut pos, 4).ok_or_else(bad)?;
                }
                "YY" | "IY" => {
                    year = u32::from(now.year) / 100 * 100
                        + digits(&s, &mut pos, 2).ok_or_else(bad)?;
                }
                "RR" => {
                    let y = digits(&s, &mut pos, 2).ok_or_else(bad)?;
                    year = if y < 50 { 2000 + y } else { 1900 + y };
                }
                "MM" => month = digits(&s, &mut pos, 2).ok_or_else(bad)?,
                "DD" => day = digits(&s, &mut pos, 2).ok_or_else(bad)?,
                "DDD" => doy = Some(digits(&s, &mut pos, 3).ok_or_else(bad)?),
                "HH24" => hour = digits(&s, &mut pos, 2).ok_or_else(bad)?,
                "HH" | "HH12" => {
                    hour = digits(&s, &mut pos, 2).ok_or_else(bad)?;
                    pm.get_or_insert(false);
                }
                "MI" => minute = digits(&s, &mut pos, 2).ok_or_else(bad)?,
                "SS" => second = digits(&s, &mut pos, 2).ok_or_else(bad)?,
                "SSSSS" => {
                    let n = digits(&s, &mut pos, 5).ok_or_else(bad)?;
                    (hour, minute, second) = (n / 3600, n % 3600 / 60, n % 60);
                }
                "NNNNNN" | "FF" | "FF1" | "FF2" | "FF3" | "FF4" | "FF5" | "FF6" | "FF7" | "FF8"
                | "FF9" | "FF10" | "FF11" | "FF12" => {
                    let max = if e == "NNNNNN" {
                        6
                    } else {
                        e.trim_start_matches("FF").parse().unwrap_or(12)
                    };
                    let start = pos;
                    let n = digits(&s, &mut pos, max).ok_or_else(bad)?;
                    let got = u32::try_from(pos - start).expect("small");
                    picos = u64::from(n) * 10u64.pow(12 - got);
                }
                "AM" | "PM" | "A.M." | "P.M." => {
                    let rest: String = s[pos..].iter().collect::<String>().to_ascii_uppercase();
                    let (word, is_pm) = if rest.starts_with("A.M.") {
                        ("A.M.", false)
                    } else if rest.starts_with("P.M.") {
                        ("P.M.", true)
                    } else if rest.starts_with("AM") {
                        ("AM", false)
                    } else if rest.starts_with("PM") {
                        ("PM", true)
                    } else {
                        return Err(bad());
                    };
                    pos += word.len();
                    pm = Some(is_pm);
                }
                "MONTH" | "MON" => {
                    let rest: String = s[pos..].iter().collect::<String>().to_ascii_uppercase();
                    let (i, n) = MONTHS
                        .iter()
                        .enumerate()
                        .find_map(|(i, m)| {
                            if rest.starts_with(m) {
                                Some((i, m.len()))
                            } else if rest.starts_with(&m[..3]) {
                                Some((i, 3))
                            } else {
                                None
                            }
                        })
                        .ok_or_else(bad)?;
                    month = u32::try_from(i + 1).expect("small");
                    pos += n;
                }
                "DAY" | "DY" => {
                    let rest: String = s[pos..].iter().collect::<String>().to_ascii_uppercase();
                    let n = DAYS
                        .iter()
                        .find_map(|d| {
                            if rest.starts_with(d) {
                                Some(d.len())
                            } else if rest.starts_with(&d[..3]) {
                                Some(3)
                            } else {
                                None
                            }
                        })
                        .ok_or_else(bad)?;
                    pos += n;
                }
                _ => {
                    return Err(SqlError::Unsupported {
                        what: format!("TIMESTAMP_FORMAT element {e}"),
                    });
                }
            },
        }
    }
    if pos < s.len() {
        return Err(bad());
    }
    if let Some(is_pm) = pm {
        if !(1..=12).contains(&hour) {
            return Err(bad());
        }
        hour = hour % 12 + if is_pm { 12 } else { 0 };
    }
    let y = u16::try_from(year).map_err(|_| bad())?;
    let date = if let Some(n) = doy {
        let jan1 = Date::new(y, 1, 1).ok_or_else(bad)?;
        let max = if db::record::datetime::is_leap(y) {
            366
        } else {
            365
        };
        if !(1..=max).contains(&n) {
            return Err(bad());
        }
        date_from_days(days_of(jan1) + i64::from(n) - 1).ok_or_else(bad)?
    } else {
        let m = u8::try_from(month).map_err(|_| bad())?;
        let d = u8::try_from(day).map_err(|_| bad())?;
        if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
            return Err(bad());
        }
        Date::new(y, m, d).ok_or_else(bad)?
    };
    let time = Time::new(
        u8::try_from(hour).map_err(|_| bad())?,
        u8::try_from(minute).map_err(|_| bad())?,
        u8::try_from(second).map_err(|_| bad())?,
    )
    .ok_or_else(bad)?;
    debug_assert!(picos < PICOS);
    Ok(Timestamp {
        date,
        time,
        picos,
        precision: 12,
    })
}

/// A number as `VARCHAR_FORMAT` shows it with a number format; without
/// one, its plain text.
#[allow(clippy::too_many_lines)]
pub(crate) fn number_text(v: &Value, fmt: Option<&str>) -> Result<String> {
    let Some(fmt) = fmt else {
        return Ok(v.as_text());
    };
    let f = fmt.trim().to_ascii_uppercase();
    let (mut body, mut lead_sign, mut trail) = (f.as_str(), false, "");
    let mut currency = "";
    if let Some(r) = body.strip_prefix('S') {
        lead_sign = true;
        body = r;
    }
    for (p, c) in [("$", "$"), ("L", "$")] {
        if let Some(r) = body.strip_prefix(p) {
            currency = c;
            body = r;
        }
    }
    for t in ["MI", "PR", "S"] {
        if let Some(r) = body.strip_suffix(t) {
            trail = t;
            body = r;
            break;
        }
    }
    let (int_part, frac_part) = match body.find(['.', 'D']) {
        Some(i) => (&body[..i], &body[i + 1..]),
        None => (body, ""),
    };
    if !int_part.chars().all(|c| matches!(c, '9' | '0' | ',' | 'G'))
        || !frac_part.chars().all(|c| matches!(c, '9' | '0'))
    {
        return Err(SqlError::Conversion {
            what: format!("'{fmt}' is not a number format"),
        });
    }
    let scale = u8::try_from(frac_part.len()).map_err(|_| SqlError::Conversion {
        what: "too many decimal places".into(),
    })?;
    let d = v
        .as_decimal()?
        .rescale(scale, Rounding::HalfAdjust)
        .map_err(|e| SqlError::Overflow {
            what: e.to_string(),
        })?;
    let neg = d.is_negative();
    let text = d.abs().to_string();
    let (int_digits, frac_digits) = text
        .split_once('.')
        .map_or((text.as_str(), ""), |(a, b)| (a, b));
    let int_digits = int_digits.trim_start_matches('0');
    let slots = int_part.chars().filter(|c| matches!(c, '9' | '0')).count();
    if int_digits.len() > slots {
        let width = fmt.trim().len() + 1;
        return Ok("#".repeat(width));
    }
    // Digits right-aligned into the slots; before the first digit a 9
    // is a blank and a 0 (and every slot after it) a zero.
    let digs: Vec<char> = int_digits.chars().collect();
    let first_zero = int_part
        .chars()
        .filter(|c| matches!(c, '9' | '0'))
        .position(|c| c == '0');
    let mut slot = 0;
    let mut shown = false;
    let mut int_text = String::new();
    for c in int_part.chars() {
        if matches!(c, '9' | '0') {
            let k = slot;
            slot += 1;
            let from = slots - digs.len();
            if k >= from {
                int_text.push(digs[k - from]);
                shown = true;
            } else if first_zero.is_some_and(|z| k >= z) {
                int_text.push('0');
                shown = true;
            } else {
                int_text.push(' ');
            }
        } else {
            int_text.push(if shown { ',' } else { ' ' });
        }
    }
    let mut out = String::from(currency);
    // The sign's place before the digits: a blank for a positive number
    // unless S asks for a plus.
    if lead_sign {
        out.push(if neg { '-' } else { '+' });
    } else if trail.is_empty() {
        out.push(if neg { '-' } else { ' ' });
    }
    out.push_str(&int_text);
    if !frac_part.is_empty() {
        out.push('.');
        out.push_str(frac_digits);
    }
    match trail {
        "MI" => out.push(if neg { '-' } else { ' ' }),
        "S" => out.push(if neg { '-' } else { '+' }),
        "PR" => {
            if neg {
                out = format!("<{}>", out.trim_start());
            } else {
                out.push(' ');
            }
        }
        _ => {}
    }
    Ok(out)
}
