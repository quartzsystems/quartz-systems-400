// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Daylight saving rules, computed from the calendar. One hour of saving
//! in every rule here.

use crate::clock::{civil_from_days, days_from_civil};

/// A daylight saving rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DstRule {
    /// Second Sunday in March 02:00 local to first Sunday in November
    /// 02:00 local (United States and Canada since 2007).
    Us,
    /// Last Sunday in March 01:00 UTC to last Sunday in October 01:00 UTC
    /// (European Union and United Kingdom).
    Eu,
    /// First Sunday in October 02:00 standard to first Sunday in April
    /// 03:00 daylight (Australian south-eastern states).
    AuSouth,
    /// Last Sunday in September 02:00 standard to first Sunday in April
    /// 03:00 daylight (New Zealand).
    Nz,
}

const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

/// Is daylight saving in effect at `unix` for a zone with `std_offset`?
pub fn in_dst(rule: DstRule, unix: u64, std_offset: i32) -> bool {
    let unix = i64::try_from(unix).unwrap_or(i64::MAX);
    let year = civil_from_days((unix + i64::from(std_offset)).div_euclid(DAY)).0;
    let std = i64::from(std_offset);
    match rule {
        DstRule::Us => {
            let start = nth_sunday(year, 3, 2) * DAY + 2 * HOUR - std;
            let end = nth_sunday(year, 11, 1) * DAY + 2 * HOUR - (std + HOUR);
            unix >= start && unix < end
        }
        DstRule::Eu => {
            let start = last_sunday(year, 3) * DAY + HOUR;
            let end = last_sunday(year, 10) * DAY + HOUR;
            unix >= start && unix < end
        }
        DstRule::AuSouth => {
            // Southern summer: DST from October to April of the next year.
            let end = nth_sunday(year, 4, 1) * DAY + 3 * HOUR - (std + HOUR);
            let start = nth_sunday(year, 10, 1) * DAY + 2 * HOUR - std;
            unix < end || unix >= start
        }
        DstRule::Nz => {
            let end = nth_sunday(year, 4, 1) * DAY + 3 * HOUR - (std + HOUR);
            let start = last_sunday(year, 9) * DAY + 2 * HOUR - std;
            unix < end || unix >= start
        }
    }
}

/// Days since the epoch of the `n`th Sunday of a month.
fn nth_sunday(year: i64, month: u32, n: i64) -> i64 {
    let first = days_from_civil(year, month, 1);
    let first_sunday = first + (7 - weekday(first)) % 7;
    first_sunday + (n - 1) * 7
}

/// Days since the epoch of the last Sunday of a month.
fn last_sunday(year: i64, month: u32) -> i64 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let last = days_from_civil(ny, nm, 1) - 1;
    last - weekday(last)
}

/// 0 = Sunday. 1970-01-01 was a Thursday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sundays() {
        // 2026-03-08 is the second Sunday of March 2026.
        assert_eq!(nth_sunday(2026, 3, 2), days_from_civil(2026, 3, 8));
        // 2026-10-25 is the last Sunday of October 2026.
        assert_eq!(last_sunday(2026, 10), days_from_civil(2026, 10, 25));
        assert_eq!(weekday(days_from_civil(2026, 10, 10)), 6); // Saturday
    }
}
