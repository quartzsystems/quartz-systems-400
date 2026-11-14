// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Generalized Time and UTC Time (RFC 4517 §3.3.13, §3.3.34) to one
//! canonical UTC form, `YYYYMMDDHHMMSS.nnnnnnnnnZ`, whose bytes order as
//! the times do.

/// A time as seconds since 0000-03-01 (proleptic Gregorian) and
/// nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    secs: i64,
    nanos: u32,
}

fn digits(s: &[u8], n: usize) -> Option<(u32, &[u8])> {
    if s.len() < n || !s[..n].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let v = s[..n]
        .iter()
        .fold(0u32, |a, &d| a * 10 + u32::from(d - b'0'));
    Some((v, &s[n..]))
}

/// Days from 0000-03-01 to the date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        4 | 6 | 9 | 11 => 30,
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

/// The zone after the time: `Z`, `+hh[mm]`, `-hh[mm]`, or nothing when
/// `lenient` (taken as UTC). Seconds east of UTC.
fn zone(s: &[u8], lenient: bool) -> Option<i64> {
    match s.first() {
        None if lenient => Some(0),
        Some(b'Z') if s.len() == 1 => Some(0),
        Some(&c @ (b'+' | b'-')) => {
            let (h, rest) = digits(&s[1..], 2)?;
            let (m, rest) = if rest.is_empty() {
                (0, rest)
            } else {
                digits(rest, 2)?
            };
            if !rest.is_empty() || h > 23 || m > 59 {
                return None;
            }
            let off = i64::from(h * 3600 + m * 60);
            Some(if c == b'-' { -off } else { off })
        }
        _ => None,
    }
}

fn build(y: i64, mo: u32, d: u32, sod: i64, nanos: u32, off: i64) -> Option<Instant> {
    if !(1..=12).contains(&mo) || d == 0 || d > days_in_month(y, mo) {
        return None;
    }
    Some(Instant {
        secs: days_from_civil(y, mo, d) * 86_400 + sod - off,
        nanos,
    })
}

/// Parses a Generalized Time. `lenient` accepts a missing zone as UTC.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn parse_generalized(s: &[u8], lenient: bool) -> Option<Instant> {
    let (y, s) = digits(s, 4)?;
    let (mo, s) = digits(s, 2)?;
    let (d, s) = digits(s, 2)?;
    let (h, s) = digits(s, 2)?;
    if h > 23 {
        return None;
    }
    // Minutes and seconds are optional; a fraction applies to the last
    // unit given.
    let (mi, s, unit) = match digits(s, 2) {
        Some((mi, r)) => (Some(mi), r, 60u64),
        None => (None, s, 3600u64),
    };
    let (se, s, unit) = match (mi, digits(s, 2)) {
        (Some(_), Some((se, r))) => (Some(se), r, 1u64),
        _ => (None, s, unit),
    };
    if mi.is_some_and(|m| m > 59) || se.is_some_and(|x| x > 60) {
        return None;
    }
    let mut extra_nanos: u128 = 0;
    let mut s = s;
    if let Some(b'.' | b',') = s.first() {
        let n = s[1..].iter().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        let frac = &s[1..=n];
        // fraction * unit seconds, in nanoseconds; digits past 18 drop.
        let used = &frac[..frac.len().min(18)];
        let num = used
            .iter()
            .fold(0u128, |a, &c| a * 10 + u128::from(c - b'0'));
        let den = 10u128.pow(u32::try_from(used.len()).unwrap_or(18));
        extra_nanos = num * u128::from(unit) * 1_000_000_000 / den;
        s = &s[n + 1..];
    }
    let off = zone(s, lenient)?;
    let base = i64::from(h) * 3600 + i64::from(mi.unwrap_or(0)) * 60 + i64::from(se.unwrap_or(0));
    let extra_secs = i64::try_from(extra_nanos / 1_000_000_000).ok()?;
    let nanos = u32::try_from(extra_nanos % 1_000_000_000).ok()?;
    build(i64::from(y), mo, d, base + extra_secs, nanos, off)
}

/// Parses a UTC Time (`YYMMDDHHMM[SS](Z|+hhmm|-hhmm)`); years below 50
/// are 20YY.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn parse_utc(s: &[u8], lenient: bool) -> Option<Instant> {
    let (yy, s) = digits(s, 2)?;
    let (mo, s) = digits(s, 2)?;
    let (d, s) = digits(s, 2)?;
    let (h, s) = digits(s, 2)?;
    let (mi, s) = digits(s, 2)?;
    let (se, s) = match digits(s, 2) {
        Some((se, r)) => (se, r),
        None => (0, s),
    };
    if h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let off = zone(s, lenient)?;
    let y = if yy < 50 { 2000 + yy } else { 1900 + yy };
    build(
        i64::from(y),
        mo,
        d,
        i64::from(h * 3600 + mi * 60 + se),
        0,
        off,
    )
}

impl Instant {
    /// The canonical form, `YYYYMMDDHHMMSS.nnnnnnnnnZ`.
    #[must_use]
    pub fn canonical(&self) -> String {
        let days = self.secs.div_euclid(86_400);
        let sod = self.secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        format!(
            "{y:04}{m:02}{d:02}{:02}{:02}{:02}.{:09}Z",
            sod / 3600,
            (sod / 60) % 60,
            sod % 60,
            self.nanos
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(s: &str) -> String {
        parse_generalized(s.as_bytes(), false).unwrap().canonical()
    }

    #[test]
    fn forms() {
        assert_eq!(g("20261113120000Z"), "20261113120000.000000000Z");
        assert_eq!(g("2026111312Z"), "20261113120000.000000000Z");
        assert_eq!(g("202611131230Z"), "20261113123000.000000000Z");
        assert_eq!(g("20261113120000.5Z"), "20261113120000.500000000Z");
        assert_eq!(g("2026111312.5Z"), "20261113123000.000000000Z");
        assert_eq!(g("20261113120000+0100"), "20261113110000.000000000Z");
        assert_eq!(g("20261231233000-0100"), "20270101003000.000000000Z");
        assert_eq!(g("20240229000000Z"), "20240229000000.000000000Z");
    }

    #[test]
    fn bad() {
        for s in [
            "2026111312",
            "20261313120000Z",
            "20230229000000Z",
            "20261113126000Z",
            "2026111312.Z",
            "x",
        ] {
            assert!(parse_generalized(s.as_bytes(), false).is_none(), "{s}");
        }
        assert!(parse_generalized(b"20261113120000", true).is_some());
    }

    #[test]
    fn utc() {
        assert_eq!(
            parse_utc(b"2611131200Z", false).unwrap().canonical(),
            "20261113120000.000000000Z"
        );
        assert_eq!(
            parse_utc(b"991231235959Z", false).unwrap().canonical(),
            "19991231235959.000000000Z"
        );
    }

    #[test]
    fn order() {
        let a = parse_generalized(b"20261113120000Z", false).unwrap();
        let b = parse_generalized(b"20261113125959+0100", false).unwrap();
        assert!(b < a);
        assert!(b.canonical() < a.canonical());
    }
}
