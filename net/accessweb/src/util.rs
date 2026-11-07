// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dates and times as the pages show them.

/// Year, month and day of a day count since 1970-01-01.
pub fn civil(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, u32::try_from(m).unwrap_or(1), u32::try_from(d).unwrap_or(1))
}

/// Days since 1970-01-01 of a date.
pub fn days(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Seconds since 1970 of a UTC date and time.
pub fn secs(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> u64 {
    u64::try_from(days(y, mo, d) * 86_400).unwrap_or(0) + u64::from(h * 3600 + mi * 60 + s)
}

/// `secs` shifted by `offset` seconds as date (`MM/DD/YY`, `DD/MM/YY`,
/// `YYYY-MM-DD`, `DD.MM.YYYY`) and time (24 hour, or 12 with AM/PM).
#[allow(clippy::many_single_char_names)]
pub fn stamp(secs: u64, offset: i64, date: &str, ampm: bool) -> (String, String) {
    let t = i64::try_from(secs).unwrap_or(0) + offset;
    let (y, m, d) = civil(t.div_euclid(86_400));
    let r = t.rem_euclid(86_400);
    let yy = y.rem_euclid(100);
    let date = match date {
        "DD/MM/YY" => format!("{d:02}/{m:02}/{yy:02}"),
        "YYYY-MM-DD" => format!("{y:04}-{m:02}-{d:02}"),
        "DD.MM.YYYY" => format!("{d:02}.{m:02}.{y:04}"),
        _ => format!("{m:02}/{d:02}/{yy:02}"),
    };
    let (h, mi, s) = (r / 3600, r % 3600 / 60, r % 60);
    let time = if ampm {
        let h12 = if h % 12 == 0 { 12 } else { h % 12 };
        format!("{h12:02}:{mi:02}:{s:02} {}", if h < 12 { "AM" } else { "PM" })
    } else {
        format!("{h:02}:{mi:02}:{s:02}")
    };
    (date, time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        let t = secs(2026, 10, 29, 17, 5, 0);
        assert_eq!(stamp(t, -4 * 3600, "MM/DD/YY", false), ("10/29/26".into(), "13:05:00".into()));
        assert_eq!(stamp(t, 0, "YYYY-MM-DD", true), ("2026-10-29".into(), "05:05:00 PM".into()));
        assert_eq!(stamp(t, 0, "DD.MM.YYYY", false).0, "29.10.2026");
        assert_eq!(stamp(secs(2026, 1, 1, 0, 30, 0), 0, "DD/MM/YY", true).1, "12:30:00 AM");
    }
}
