// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Date pictures of the `CEE` date APIs (`YYYYMMDD`, `MM/DD/YY`,
//! `YYYY-MM-DD HH:MI:SS.999`, ...) and the Lilian calendar: day 1 is
//! 1582-10-15, Lilian seconds count from 1582-10-14 00:00:00.

use std::fmt::Write as _;

use crate::job::Civil;
use sysval::days_from_civil;

/// Days from 1970-01-01 to the Lilian epoch's day 0, 1582-10-14.
fn lilian_epoch_days() -> i64 {
    days_from_civil(1582, 10, 14)
}

/// Picture tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Y4,
    Y2,
    Mon,
    Mmm,
    Day,
    Yday,
    Hour,
    Min,
    Sec,
    Ms,
    Lit(u8),
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn tokens(picture: &str) -> Vec<Tok> {
    let b = picture.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let rest = &picture[i..];
        let (t, n) = if rest.starts_with("YYYY") {
            (Tok::Y4, 4)
        } else if rest.starts_with("YY") {
            (Tok::Y2, 2)
        } else if rest.starts_with("Mmm") {
            (Tok::Mmm, 3)
        } else if rest.starts_with("MM") {
            (Tok::Mon, 2)
        } else if rest.starts_with("DDD") {
            (Tok::Yday, 3)
        } else if rest.starts_with("DD") {
            (Tok::Day, 2)
        } else if rest.starts_with("HH") {
            (Tok::Hour, 2)
        } else if rest.starts_with("MI") {
            (Tok::Min, 2)
        } else if rest.starts_with("SS") {
            (Tok::Sec, 2)
        } else if rest.starts_with("999") {
            (Tok::Ms, 3)
        } else {
            (Tok::Lit(b[i]), 1)
        };
        v.push(t);
        i += n;
    }
    v
}

/// Whether a picture holds a date, a time or both.
#[must_use]
pub fn valid(picture: &str) -> bool {
    let t = tokens(picture);
    let has = |k: Tok| t.contains(&k);
    let date = (has(Tok::Y4) || has(Tok::Y2))
        && ((has(Tok::Mon) || has(Tok::Mmm)) && has(Tok::Day) || has(Tok::Yday));
    let time = has(Tok::Hour) || has(Tok::Min) || has(Tok::Sec);
    (date || time) && !picture.is_empty()
}

/// Render a time with a picture; `None` for a bad picture.
#[must_use]
pub fn render(c: Civil, picture: &str) -> Option<String> {
    if !valid(picture) {
        return None;
    }
    let mut s = String::new();
    for t in tokens(picture) {
        match t {
            Tok::Y4 => {
                let _ = write!(s, "{:04}", c.year);
            }
            Tok::Y2 => {
                let _ = write!(s, "{:02}", c.year.rem_euclid(100));
            }
            Tok::Mon => {
                let _ = write!(s, "{:02}", c.month);
            }
            Tok::Mmm => s.push_str(MONTHS.get(c.month as usize - 1).copied().unwrap_or("???")),
            Tok::Day => {
                let _ = write!(s, "{:02}", c.day);
            }
            Tok::Yday => {
                let _ = write!(s, "{:03}", c.yday());
            }
            Tok::Hour => {
                let _ = write!(s, "{:02}", c.hour);
            }
            Tok::Min => {
                let _ = write!(s, "{:02}", c.min);
            }
            Tok::Sec => {
                let _ = write!(s, "{:02}", c.sec);
            }
            Tok::Ms => {
                let _ = write!(s, "{:03}", c.ms);
            }
            Tok::Lit(b) => s.push(char::from(b)),
        }
    }
    Some(s)
}

/// Parse a time with a picture and a century window: a two-digit
/// year `yy` is in the century that keeps it within `century` years
/// of the current one. `None` when the text does not fit.
#[must_use]
pub fn parse(text: &str, picture: &str, century: i64, now_year: i64) -> Option<Civil> {
    if !valid(picture) {
        return None;
    }
    let b = text.as_bytes();
    let mut i = 0;
    let mut c = Civil {
        year: now_year,
        month: 1,
        day: 1,
        ..Civil::default()
    };
    let mut yday = None;
    let num = |i: &mut usize, n: usize| -> Option<u32> {
        let s = text.get(*i..*i + n)?;
        *i += n;
        s.parse().ok()
    };
    for t in tokens(picture) {
        match t {
            Tok::Y4 => c.year = i64::from(num(&mut i, 4)?),
            Tok::Y2 => {
                let yy = i64::from(num(&mut i, 2)?);
                let base = now_year - century;
                let mut y = base - base.rem_euclid(100) + yy;
                if y < base {
                    y += 100;
                }
                c.year = y;
            }
            Tok::Mon => c.month = num(&mut i, 2)?,
            Tok::Mmm => {
                let s = text.get(i..i + 3)?;
                c.month = u32::try_from(MONTHS.iter().position(|m| m.eq_ignore_ascii_case(s))? + 1)
                    .ok()?;
                i += 3;
            }
            Tok::Day => c.day = num(&mut i, 2)?,
            Tok::Yday => yday = Some(num(&mut i, 3)?),
            Tok::Hour => c.hour = num(&mut i, 2)?,
            Tok::Min => c.min = num(&mut i, 2)?,
            Tok::Sec => c.sec = num(&mut i, 2)?,
            Tok::Ms => c.ms = num(&mut i, 3)?,
            Tok::Lit(l) => {
                if b.get(i) != Some(&l) {
                    return None;
                }
                i += 1;
            }
        }
    }
    if let Some(d) = yday {
        let y = c.year;
        let base = Civil::of_yday(y, d);
        c.month = base.month;
        c.day = base.day;
    }
    let ok = (1..=12).contains(&c.month)
        && c.day >= 1
        && c.day <= days_in_month(c.year, c.month)
        && c.hour < 24
        && c.min < 60
        && c.sec < 60;
    ok.then_some(c)
}

/// Days in a month.
#[must_use]
pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        _ => 28,
    }
}

/// The Lilian day of a date.
#[must_use]
pub fn lilian_of(c: Civil) -> i32 {
    i32::try_from(days_from_civil(c.year, c.month, c.day) - lilian_epoch_days()).unwrap_or(0)
}

/// The date of a Lilian day.
#[must_use]
pub fn civil_of_lilian(lilian: i32) -> Civil {
    let days = i64::from(lilian) + lilian_epoch_days();
    let (y, m, d) = sysval::civil_from_days(days);
    Civil {
        year: y,
        month: m,
        day: d,
        ..Civil::default()
    }
}

/// Lilian seconds of a time.
#[must_use]
pub fn seconds_of(c: Civil) -> f64 {
    let day = f64::from(lilian_of(c)) * 86_400.0;
    day + f64::from(c.hour) * 3600.0
        + f64::from(c.min) * 60.0
        + f64::from(c.sec)
        + f64::from(c.ms) / 1000.0
}

/// The time of Lilian seconds.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn civil_of_seconds(s: f64) -> Civil {
    let whole = s.floor();
    let day = (whole / 86_400.0).floor();
    let rem = (whole - day * 86_400.0) as u32;
    let mut c = civil_of_lilian(day as i32);
    c.hour = rem / 3600;
    c.min = rem % 3600 / 60;
    c.sec = rem % 60;
    c.ms = ((s - whole) * 1000.0).round() as u32;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lilian_epoch_and_round_trips() {
        let first = Civil {
            year: 1582,
            month: 10,
            day: 15,
            ..Civil::default()
        };
        assert_eq!(lilian_of(first), 1);
        let d = Civil {
            year: 2026,
            month: 10,
            day: 21,
            hour: 13,
            min: 5,
            sec: 9,
            ms: 250,
        };
        assert_eq!(civil_of_lilian(lilian_of(d)).day, 21);
        assert_eq!(civil_of_seconds(seconds_of(d)), d);
        assert_eq!(
            render(d, "YYYY-MM-DD HH:MI:SS.999").unwrap(),
            "2026-10-21 13:05:09.250"
        );
        assert_eq!(render(d, "DD Mmm YY").unwrap(), "21 Oct 26");
        let p = parse("10/21/26", "MM/DD/YY", 80, 2026).unwrap();
        assert_eq!((p.year, p.month, p.day), (2026, 10, 21));
        let p = parse("10/21/99", "MM/DD/YY", 80, 2026).unwrap();
        assert_eq!(p.year, 1999);
        assert!(parse("13/01/2026", "MM/DD/YYYY", 80, 2026).is_none());
        assert!(!valid("hello"));
        assert_eq!(parse("2026294", "YYYYDDD", 80, 2026).unwrap().month, 10);
    }
}
