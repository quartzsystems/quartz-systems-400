// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dates the server writes and reads, in UTC: `Last-Modified` and
//! `If-Modified-Since` (IMF-fixdate), the access log's `%t`, the error
//! log's stamp and a rotated log's day suffix.

const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A moment broken down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    /// The year.
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub min: u32,
    /// 0 to 59.
    pub sec: u32,
    /// 0 is Thursday (1970-01-01 was one).
    pub weekday: usize,
}

fn small(v: i64) -> u32 {
    u32::try_from(v).unwrap_or(0)
}

/// `secs` since 1970 broken down.
pub fn civil(secs: u64) -> Civil {
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Civil {
        year: yoe + era * 400 + i64::from(m <= 2),
        month: small(m),
        day: small(d),
        hour: small(rem / 3600),
        min: small(rem % 3600 / 60),
        sec: small(rem % 60),
        weekday: usize::try_from(days.rem_euclid(7)).unwrap_or(0),
    }
}

/// Days since 1970 of a date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn month_name(m: u32) -> &'static str {
    MONTHS[usize::try_from(m.saturating_sub(1)).unwrap_or(0).min(11)]
}

/// IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`.
pub fn http_date(secs: u64) -> String {
    let c = civil(secs);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAYS[c.weekday],
        c.day,
        month_name(c.month),
        c.year,
        c.hour,
        c.min,
        c.sec
    )
}

/// Parse an IMF-fixdate; `None` for anything else.
pub fn parse_http_date(s: &str) -> Option<u64> {
    let rest = s.trim().split_once(", ")?.1;
    let mut p = rest.split(' ');
    let day: u32 = p.next()?.parse().ok()?;
    let mname = p.next()?;
    let month = u32::try_from(MONTHS.iter().position(|m| *m == mname)? + 1).ok()?;
    let year: i64 = p.next()?.parse().ok()?;
    let mut t = p.next()?.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let mi: i64 = t.next()?.parse().ok()?;
    let se: i64 = t.next()?.parse().ok()?;
    if p.next()? != "GMT" || day == 0 || day > 31 || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + h * 3600 + mi * 60 + se;
    u64::try_from(secs).ok()
}

/// The access log's `%t`: `[01/Nov/2026:12:00:00 +0000]`.
pub fn clf(secs: u64) -> String {
    let c = civil(secs);
    format!(
        "[{:02}/{}/{:04}:{:02}:{:02}:{:02} +0000]",
        c.day,
        month_name(c.month),
        c.year,
        c.hour,
        c.min,
        c.sec
    )
}

/// The error log's stamp: `Sun Nov 01 12:00:00 2026`.
pub fn error_stamp(secs: u64) -> String {
    let c = civil(secs);
    format!(
        "{} {} {:02} {:02}:{:02}:{:02} {:04}",
        DAYS[c.weekday],
        month_name(c.month),
        c.day,
        c.hour,
        c.min,
        c.sec,
        c.year
    )
}

/// A rotated log's suffix: `20261101123000` (when it rotated or was
/// last written).
pub fn stamp(secs: u64) -> String {
    let c = civil(secs);
    format!("{:04}{:02}{:02}{:02}{:02}{:02}", c.year, c.month, c.day, c.hour, c.min, c.sec)
}

/// Whether `s` is a rotated log's suffix: 14 digits (8 from logs
/// rotated by day alone).
pub fn is_stamp(s: &str) -> bool {
    (s.len() == 14 || s.len() == 8) && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses() {
        assert_eq!(http_date(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
        assert_eq!(parse_http_date("Sunday, 06-Nov-94 08:49:37 GMT"), None);
        assert_eq!(parse_http_date(&http_date(1_793_232_000)), Some(1_793_232_000));
        assert_eq!(clf(1_793_232_000), "[29/Oct/2026:00:00:00 +0000]");
        assert_eq!(error_stamp(1_793_232_000), "Thu Oct 29 00:00:00 2026");
        assert_eq!(stamp(1_793_232_000 + 3723), "20261029010203");
        assert!(is_stamp("20261029010203") && is_stamp("20261029") && !is_stamp("2026102x"));
    }
}
