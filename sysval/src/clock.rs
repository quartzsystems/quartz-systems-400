// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calendar arithmetic and the clock-derived system values.

/// Local calendar time broken down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    /// Full year.
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub minute: u32,
    /// 0 to 59.
    pub second: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    /// 1 to 366.
    pub yearday: u32,
}

impl LocalTime {
    /// Break down `unix` shifted by `offset` seconds.
    pub fn from_unix(unix: u64, offset: i32) -> Self {
        let t = i64::try_from(unix)
            .unwrap_or(i64::MAX)
            .saturating_add(i64::from(offset));
        let days = t.div_euclid(86_400);
        let secs = t.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let yearday = u32::try_from(days - days_from_civil(year, 1, 1) + 1).unwrap_or(1);
        Self {
            year,
            month,
            day,
            hour: u32::try_from(secs / 3600).unwrap_or(0),
            minute: u32::try_from((secs % 3600) / 60).unwrap_or(0),
            second: u32::try_from(secs % 60).unwrap_or(0),
            weekday: u32::try_from((days + 4).rem_euclid(7)).unwrap_or(0),
            yearday,
        }
    }

    /// QDATE in the given format and separator-free form (six digits).
    pub fn qdate(&self, fmt: &str) -> String {
        let yy = self.year.rem_euclid(100);
        match fmt {
            "YMD" => format!("{yy:02}{:02}{:02}", self.month, self.day),
            "DMY" => format!("{:02}{:02}{yy:02}", self.day, self.month),
            "JUL" => format!("{yy:02}{:03}", self.yearday),
            _ => format!("{:02}{:02}{yy:02}", self.month, self.day),
        }
    }

    /// QDAY: day of month, or day of year under JUL.
    pub fn qday(&self, fmt: &str) -> String {
        if fmt == "JUL" {
            format!("{:03}", self.yearday)
        } else {
            format!("{:02}", self.day)
        }
    }

    /// QTIME as HHMMSS.
    pub fn qtime(&self) -> String {
        format!("{:02}{:02}{:02}", self.hour, self.minute, self.second)
    }

    /// QDAYOFWEEK.
    pub fn qdayofweek(&self) -> &'static str {
        ["*SUN", "*MON", "*TUE", "*WED", "*THU", "*FRI", "*SAT"][self.weekday as usize % 7]
    }

    /// QDATETIME as `YYYYMMDDHHMMSSmmmuuu` (sub-seconds zero).
    pub fn qdatetime(&self) -> String {
        format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}000000",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// QCENTURY: 0 for 19xx, 1 for 20xx.
    pub fn qcentury(&self) -> &'static str {
        if self.year < 2000 { "0" } else { "1" }
    }
}

/// Civil date to Unix seconds, UTC.
pub fn civil_to_unix(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> u64 {
    let secs = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3600
        + i64::from(min) * 60
        + i64::from(sec);
    u64::try_from(secs).unwrap_or(0)
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 1970-01-01 to `(year, month, day)`.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 10, 10),
            (2099, 12, 31),
            (1999, 3, 1),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn derived_values() {
        let t = LocalTime::from_unix(civil_to_unix(2026, 10, 10, 15, 4, 5), -4 * 3600);
        assert_eq!((t.year, t.month, t.day, t.hour), (2026, 10, 10, 11));
        assert_eq!(t.qdate("MDY"), "101026");
        assert_eq!(t.qdate("YMD"), "261010");
        assert_eq!(t.qdate("DMY"), "101026");
        assert_eq!(t.qdate("JUL"), "26283");
        assert_eq!(t.qtime(), "110405");
        assert_eq!(t.qdayofweek(), "*SAT");
        assert_eq!(t.qcentury(), "1");
        assert_eq!(t.qdatetime(), "20261010110405000000");
    }
}
