// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Date` header: RFC 9110's IMF-fixdate (`Sun, 06 Nov 1994
//! 08:49:37 GMT`) from seconds since 1970.

const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Year, month (1-12) and day of a day number since 1970-01-01.
fn civil(days: i64) -> (i64, usize, i64) {
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
    (y, usize::try_from(m - 1).unwrap_or(0), d)
}

/// `secs` since 1970 as an IMF-fixdate.
pub fn imf_fixdate(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    let wd = DAYS[usize::try_from(days.rem_euclid(7)).unwrap_or(0)];
    format!(
        "{wd}, {d:02} {} {y:04} {:02}:{:02}:{:02} GMT",
        MONTHS[m],
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Now, as an IMF-fixdate.
pub fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    imf_fixdate(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(imf_fixdate(0), "Thu, 01 Jan 1970 00:00:00 GMT");
        assert_eq!(imf_fixdate(784_111_777), "Sun, 06 Nov 1994 08:49:37 GMT");
        assert_eq!(imf_fixdate(951_782_400), "Tue, 29 Feb 2000 00:00:00 GMT");
        assert_eq!(imf_fixdate(1_793_232_000), "Thu, 29 Oct 2026 00:00:00 GMT");
    }
}
