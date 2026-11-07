// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Small conversions the pages share.

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
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
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

/// `MM/DD/YY` and `HH:MM:SS` of `secs` shifted by `offset` seconds.
#[allow(clippy::many_single_char_names)]
pub fn mdy(secs: u64, offset: i64) -> (String, String) {
    let t = i64::try_from(secs).unwrap_or(0) + offset;
    let (y, m, d) = civil(t.div_euclid(86_400));
    let r = t.rem_euclid(86_400);
    (
        format!("{m:02}/{d:02}/{:02}", y.rem_euclid(100)),
        format!("{:02}:{:02}:{:02}", r / 3600, r % 3600 / 60, r % 60),
    )
}

/// Bytes as the pages show sizes (`212 MB`, `14 KB`).
pub fn size(bytes: u64) -> String {
    const K: u64 = 1024;
    if bytes >= K * K * K {
        format!("{} GB", bytes / (K * K * K))
    } else if bytes >= K * K {
        format!("{} MB", bytes / (K * K))
    } else if bytes >= K {
        format!("{} KB", bytes / K)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let t = secs(2026, 10, 29, 6, 0, 1);
        assert_eq!(mdy(t, 0), ("10/29/26".into(), "06:00:01".into()));
        assert_eq!(mdy(t, -7 * 3600), ("10/28/26".into(), "23:00:01".into()));
        assert_eq!(civil(days(2000, 2, 29)), (2000, 2, 29));
        assert_eq!(size(212 * 1024 * 1024 + 5), "212 MB");
    }
}
