// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A moment as a job shows it: the date in its `DATFMT` with its
//! `DATSEP`, the time with its `TIMSEP`, local to the system's time
//! zone (`QTIMZON`).

/// Days since 1970 as year, month, day.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn day_of_year(y: i64, m: i64, d: i64) -> i64 {
    const BEFORE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let idx = usize::try_from(m - 1).unwrap_or(0).min(11);
    BEFORE[idx] + d + i64::from(m > 2 && leap(y))
}

/// The separator a job attribute names; a blank one is a blank.
fn sep(value: &str, dft: char) -> char {
    value.chars().next().unwrap_or(dft)
}

/// `secs` (UTC) at `offset` seconds east of UTC, as `(date, time)`.
pub fn stamp(secs: u64, offset: i32, datfmt: &str, datsep: &str, timsep: &str) -> (String, String) {
    let local = i64::try_from(secs).unwrap_or(0) + i64::from(offset);
    let (days, s) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    let ds = sep(datsep, '/');
    let yy = y % 100;
    let date = match datfmt.trim() {
        "*YMD" => format!("{yy:02}{ds}{m:02}{ds}{d:02}"),
        "*DMY" => format!("{d:02}{ds}{m:02}{ds}{yy:02}"),
        "*JUL" => format!("{yy:02}{ds}{:03}", day_of_year(y, m, d)),
        _ => format!("{m:02}{ds}{d:02}{ds}{yy:02}"),
    };
    let ts = sep(timsep, ':');
    let time = format!("{:02}{ts}{:02}{ts}{:02}", s / 3600, s % 3600 / 60, s % 60);
    (date, time)
}

/// The offset of time zone `zone` (a `QTIMZON` value) at `secs`; `None`
/// for a zone this system does not know.
pub fn zone_offset(zone: &str, secs: u64) -> Option<i32> {
    sysval::tz::find(zone.trim()).map(|z| z.offset_at(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-29 14:02:17 UTC.
    const T: u64 = 1_793_282_537;

    #[test]
    fn formats() {
        assert_eq!(stamp(T, 0, "*MDY", "/", ":"), ("10/29/26".into(), "14:02:17".into()));
        assert_eq!(stamp(T, 0, "*YMD", "-", "."), ("26-10-29".into(), "14.02.17".into()));
        assert_eq!(stamp(T, 0, "*DMY", ".", ":").0, "29.10.26");
        assert_eq!(stamp(T, 0, "*JUL", "/", ":").0, "26/302");
        assert_eq!(stamp(T, -5 * 3600, "*MDY", "/", ":"), ("10/29/26".into(), "09:02:17".into()));
        assert_eq!(stamp(T, 10 * 3600, "*MDY", "/", ":"), ("10/30/26".into(), "00:02:17".into()));
        assert_eq!(stamp(T, 0, "*MDY", " ", ":").0, "10 29 26");
        assert_eq!(zone_offset("QN0000UTC", T), Some(0));
        assert_eq!(zone_offset("NOWHERE", T), None);
    }
}
