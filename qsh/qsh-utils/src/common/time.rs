// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Civil time without a zone database: the proleptic Gregorian calendar,
//! POSIX `TZ` strings (`EST5EDT,M3.2.0,M11.1.0`) and `strftime`.

use qsh::sys::UtilCtx;

/// A broken-down time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tm {
    /// The year.
    pub year: i64,
    /// Month, 1 to 12.
    pub mon: u32,
    /// Day of the month, 1 to 31.
    pub mday: u32,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute.
    pub min: u32,
    /// Second.
    pub sec: u32,
    /// Day of the week, 0 is Sunday.
    pub wday: u32,
    /// Day of the year, 0 to 365.
    pub yday: u32,
    /// Seconds east of UTC.
    pub gmtoff: i64,
    /// The zone's abbreviation.
    pub zone: String,
    /// Daylight saving time in effect.
    pub isdst: bool,
    /// Seconds since the epoch.
    pub epoch: i64,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Is a year a leap year?
pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days in a month.
pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

/// Days since the epoch of a civil date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A civil date from days since the epoch: (year, month, day).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Seconds since the epoch of a civil time taken as UTC.
pub fn to_epoch(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    days_from_civil(y, mo, d) * 86400 + i64::from(h) * 3600 + i64::from(mi) * 60 + i64::from(s)
}

/// Break an epoch time down in UTC.
pub fn utc(epoch: i64) -> Tm {
    with_offset(epoch, 0, "UTC", false)
}

fn with_offset(epoch: i64, gmtoff: i64, zone: &str, isdst: bool) -> Tm {
    let t = epoch + gmtoff;
    let days = t.div_euclid(86400);
    let rem = t.rem_euclid(86400);
    let (year, mon, mday) = civil_from_days(days);
    let yday = (days - days_from_civil(year, 1, 1)) as u32;
    Tm {
        year,
        mon,
        mday,
        hour: (rem / 3600) as u32,
        min: (rem % 3600 / 60) as u32,
        sec: (rem % 60) as u32,
        wday: (days + 4).rem_euclid(7) as u32,
        yday,
        gmtoff,
        zone: zone.to_string(),
        isdst,
        epoch,
    }
}

/// When daylight saving time starts or ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// `Jn`: day 1 to 365, February 29 never counted.
    Julian(u32),
    /// `n`: day 0 to 365 counting February 29.
    Day(u32),
    /// `Mm.w.d`: month, week (5 is the last), weekday.
    Month(u32, u32, u32),
}

/// A time zone from a `TZ` string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    /// The standard abbreviation.
    pub std_name: String,
    /// Standard time's seconds east of UTC.
    pub std_off: i64,
    /// Daylight time: name, offset, start rule and time, end rule and
    /// time (the times in seconds after local midnight).
    pub dst: Option<(String, i64, Rule, i64, Rule, i64)>,
}

impl Zone {
    /// UTC.
    pub fn utc() -> Self {
        Self {
            std_name: "UTC".to_string(),
            std_off: 0,
            dst: None,
        }
    }

    /// The zone of a utility: `TZ` in its environment, else UTC.
    pub fn of(cx: &UtilCtx<'_>) -> Self {
        cx.getenv("TZ")
            .and_then(parse_tz)
            .unwrap_or_else(Self::utc)
    }

    /// Break an epoch time down in this zone.
    pub fn local(&self, epoch: i64) -> Tm {
        if let Some((name, off, start, st, end, et)) = &self.dst {
            let year = civil_from_days((epoch + self.std_off).div_euclid(86400)).0;
            let start_at = rule_epoch(*start, year) + st - self.std_off;
            let end_at = rule_epoch(*end, year) + et - off;
            let in_dst = if start_at < end_at {
                epoch >= start_at && epoch < end_at
            } else {
                epoch >= start_at || epoch < end_at
            };
            if in_dst {
                return with_offset(epoch, *off, name, true);
            }
        }
        with_offset(epoch, self.std_off, &self.std_name, false)
    }
}

/// Midnight (UTC seconds) of a rule's day in a year.
fn rule_epoch(rule: Rule, year: i64) -> i64 {
    let day = match rule {
        Rule::Julian(n) => {
            let mut d = i64::from(n) - 1;
            if is_leap(year) && n >= 60 {
                d += 1;
            }
            days_from_civil(year, 1, 1) + d
        }
        Rule::Day(n) => days_from_civil(year, 1, 1) + i64::from(n),
        Rule::Month(m, w, d) => {
            let first = days_from_civil(year, m, 1);
            let first_wday = (first + 4).rem_euclid(7) as u32;
            let mut day = first + i64::from((d + 7 - first_wday) % 7);
            day += i64::from(w - 1) * 7;
            let last = days_from_civil(year, m, days_in_month(year, m));
            while day > last {
                day -= 7;
            }
            day
        }
    };
    day * 86400
}

fn parse_name(s: &str) -> Option<(String, &str)> {
    if let Some(rest) = s.strip_prefix('<') {
        let end = rest.find('>')?;
        return Some((rest[..end].to_string(), &rest[end + 1..]));
    }
    let n = s.bytes().take_while(u8::is_ascii_alphabetic).count();
    if n < 3 {
        return None;
    }
    Some((s[..n].to_string(), &s[n..]))
}

/// `[+-]hh[:mm[:ss]]` in seconds (positive as written), and the rest.
fn parse_hms(s: &str) -> Option<(i64, &str)> {
    let (sign, s) = match s.as_bytes().first() {
        Some(b'-') => (-1, &s[1..]),
        Some(b'+') => (1, &s[1..]),
        _ => (1, s),
    };
    let mut rest = s;
    let mut parts = [0i64; 3];
    for (i, p) in parts.iter_mut().enumerate() {
        if i > 0 {
            match rest.strip_prefix(':') {
                Some(r) => rest = r,
                None => break,
            }
        }
        let n = rest.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            if i == 0 {
                return None;
            }
            break;
        }
        *p = rest[..n].parse().ok()?;
        rest = &rest[n..];
    }
    Some((sign * (parts[0] * 3600 + parts[1] * 60 + parts[2]), rest))
}

fn parse_rule(s: &str) -> Option<(Rule, i64, &str)> {
    let (rule, rest) = if let Some(r) = s.strip_prefix('J') {
        let n = r.bytes().take_while(u8::is_ascii_digit).count();
        (Rule::Julian(r[..n].parse().ok()?), &r[n..])
    } else if let Some(r) = s.strip_prefix('M') {
        let mut it = r.splitn(3, '.');
        let m: u32 = it.next()?.parse().ok()?;
        let w: u32 = it.next()?.parse().ok()?;
        let d_str = it.next()?;
        let n = d_str.bytes().take_while(u8::is_ascii_digit).count();
        let d: u32 = d_str[..n].parse().ok()?;
        if !(1..=12).contains(&m) || !(1..=5).contains(&w) || d > 6 {
            return None;
        }
        (Rule::Month(m, w, d), &d_str[n..])
    } else {
        let n = s.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        (Rule::Day(s[..n].parse().ok()?), &s[n..])
    };
    let (time, rest) = match rest.strip_prefix('/') {
        Some(r) => parse_hms(r)?,
        None => (7200, rest),
    };
    Some((rule, time, rest))
}

/// Parse a POSIX `TZ` string.
pub fn parse_tz(s: &str) -> Option<Zone> {
    let s = s.strip_prefix(':').unwrap_or(s);
    let (std_name, rest) = parse_name(s)?;
    let (off, rest) = parse_hms(rest)?;
    let std_off = -off;
    if rest.is_empty() {
        return Some(Zone {
            std_name,
            std_off,
            dst: None,
        });
    }
    let (dst_name, rest) = parse_name(rest)?;
    let (dst_off, rest) = match parse_hms(rest) {
        Some((o, r)) => (-o, r),
        None => (std_off + 3600, rest),
    };
    let (start, st, end, et) = if rest.is_empty() {
        (Rule::Month(3, 2, 0), 7200, Rule::Month(11, 1, 0), 7200)
    } else {
        let r = rest.strip_prefix(',')?;
        let (start, st, r) = parse_rule(r)?;
        let r = r.strip_prefix(',')?;
        let (end, et, r) = parse_rule(r)?;
        if !r.is_empty() {
            return None;
        }
        (start, st, end, et)
    };
    Some(Zone {
        std_name,
        std_off,
        dst: Some((dst_name, dst_off, start, st, end, et)),
    })
}

/// ISO 8601 week-based year and week number.
fn iso_week(tm: &Tm) -> (i64, u32) {
    let wday = if tm.wday == 0 { 7 } else { tm.wday };
    let week = (i64::from(tm.yday) + 1 - i64::from(wday) + 10) / 7;
    if week < 1 {
        let py = tm.year - 1;
        let dec31 = utc(to_epoch(py, 12, 31, 0, 0, 0));
        return iso_week(&dec31);
    }
    if week == 53 {
        let days = if is_leap(tm.year) { 366 } else { 365 };
        let jan1_wday = (i64::from(tm.wday) - i64::from(tm.yday)).rem_euclid(7);
        let jan1 = if jan1_wday == 0 { 7 } else { jan1_wday };
        // The year has 53 weeks only when it starts on Thursday, or on
        // Wednesday in a leap year.
        if !(jan1 == 4 || (jan1 == 3 && days == 366)) {
            return (tm.year + 1, 1);
        }
    }
    (tm.year, week as u32)
}

/// Format a time as `strftime` does.
pub fn strftime(fmt: &str, tm: &Tm) -> String {
    let mut out = String::new();
    let mut it = fmt.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(c) = it.next() else {
            out.push('%');
            break;
        };
        let h12 = match tm.hour % 12 {
            0 => 12,
            h => h,
        };
        match c {
            'a' => out.push_str(&DAYS[tm.wday as usize][..3]),
            'A' => out.push_str(DAYS[tm.wday as usize]),
            'b' | 'h' => out.push_str(&MONTHS[tm.mon as usize - 1][..3]),
            'B' => out.push_str(MONTHS[tm.mon as usize - 1]),
            'c' => out.push_str(&strftime("%a %b %e %H:%M:%S %Y", tm)),
            'C' => out.push_str(&format!("{:02}", tm.year.div_euclid(100))),
            'd' => out.push_str(&format!("{:02}", tm.mday)),
            'D' => out.push_str(&strftime("%m/%d/%y", tm)),
            'e' => out.push_str(&format!("{:2}", tm.mday)),
            'F' => out.push_str(&strftime("%Y-%m-%d", tm)),
            'g' => out.push_str(&format!("{:02}", iso_week(tm).0.rem_euclid(100))),
            'G' => out.push_str(&iso_week(tm).0.to_string()),
            'H' => out.push_str(&format!("{:02}", tm.hour)),
            'I' => out.push_str(&format!("{h12:02}")),
            'j' => out.push_str(&format!("{:03}", tm.yday + 1)),
            'm' => out.push_str(&format!("{:02}", tm.mon)),
            'M' => out.push_str(&format!("{:02}", tm.min)),
            'n' => out.push('\n'),
            'p' => out.push_str(if tm.hour < 12 { "AM" } else { "PM" }),
            'r' => out.push_str(&strftime("%I:%M:%S %p", tm)),
            'R' => out.push_str(&strftime("%H:%M", tm)),
            's' => out.push_str(&tm.epoch.to_string()),
            'S' => out.push_str(&format!("{:02}", tm.sec)),
            't' => out.push('\t'),
            'T' => out.push_str(&strftime("%H:%M:%S", tm)),
            'u' => out.push_str(&(if tm.wday == 0 { 7 } else { tm.wday }).to_string()),
            'U' => out.push_str(&format!("{:02}", (tm.yday + 7 - tm.wday) / 7)),
            'V' => out.push_str(&format!("{:02}", iso_week(tm).1)),
            'w' => out.push_str(&tm.wday.to_string()),
            'W' => out.push_str(&format!("{:02}", (tm.yday + 7 - (tm.wday + 6) % 7) / 7)),
            'x' => out.push_str(&strftime("%m/%d/%y", tm)),
            'X' => out.push_str(&strftime("%H:%M:%S", tm)),
            'y' => out.push_str(&format!("{:02}", tm.year.rem_euclid(100))),
            'Y' => out.push_str(&tm.year.to_string()),
            'z' => {
                let sign = if tm.gmtoff < 0 { '-' } else { '+' };
                let a = tm.gmtoff.abs();
                out.push_str(&format!("{sign}{:02}{:02}", a / 3600, a % 3600 / 60));
            }
            'Z' => out.push_str(&tm.zone),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

/// The date column of `ls -l`: `Mon DD HH:MM` within six months of
/// now, else `Mon DD  YYYY`.
pub fn ls_date(mtime: i64, now: i64, zone: &Zone) -> String {
    let tm = zone.local(mtime);
    let recent = mtime <= now + 3600 && now - mtime < 182 * 86400 + 43200;
    if recent {
        strftime("%b %e %H:%M", &tm)
    } else {
        strftime("%b %e  %Y", &tm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_round_trips() {
        for &d in &[-719_468, -1, 0, 1, 19_000, 2_000_000] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let t = utc(1_790_000_000);
        assert_eq!((t.year, t.mon, t.mday), (2026, 9, 21));
        assert_eq!(strftime("%a %b %e %H:%M:%S %Z %Y", &t), "Mon Sep 21 14:13:20 UTC 2026");
        assert_eq!(strftime("%j %U %W %u %w", &t), "264 38 38 1 1");
    }

    #[test]
    fn tz_strings() {
        let z = parse_tz("EST5EDT,M3.2.0,M11.1.0").unwrap();
        assert_eq!(z.std_off, -5 * 3600);
        // 2026-07-01 12:00 UTC is EDT.
        let t = z.local(to_epoch(2026, 7, 1, 12, 0, 0));
        assert_eq!((t.hour, t.zone.as_str(), t.isdst), (8, "EDT", true));
        // 2026-01-01 12:00 UTC is EST.
        let t = z.local(to_epoch(2026, 1, 1, 12, 0, 0));
        assert_eq!((t.hour, t.zone.as_str()), (7, "EST"));
        // The switch: 2026-03-08 at 02:00 EST = 07:00 UTC.
        let before = z.local(to_epoch(2026, 3, 8, 6, 59, 59));
        let after = z.local(to_epoch(2026, 3, 8, 7, 0, 0));
        assert!(!before.isdst && after.isdst);
        let z = parse_tz("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        assert_eq!(z.std_off, 3600);
        let t = z.local(to_epoch(2026, 7, 1, 12, 0, 0));
        assert_eq!((t.hour, t.zone.as_str()), (14, "CEST"));
        assert_eq!(parse_tz("UTC0").unwrap().std_off, 0);
        assert_eq!(parse_tz("<+09>-9").unwrap().std_off, 9 * 3600);
        assert!(parse_tz("x").is_none());
    }

    #[test]
    fn iso_weeks() {
        // 2021-01-01 is Friday: ISO week 53 of 2020.
        let t = utc(to_epoch(2021, 1, 1, 0, 0, 0));
        assert_eq!(strftime("%G-%V", &t), "2020-53");
        // 2024-12-30 is Monday: week 1 of 2025.
        let t = utc(to_epoch(2024, 12, 30, 0, 0, 0));
        assert_eq!(strftime("%G-%V", &t), "2025-01");
        let t = utc(to_epoch(2026, 10, 29, 0, 0, 0));
        assert_eq!(strftime("%G-%V %g", &t), "2026-44 26");
    }
}
