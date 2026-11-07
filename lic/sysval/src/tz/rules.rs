// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A daylight saving rule as a time zone description holds it: the
//! month, weekday and week of its start and end, each at a local wall
//! clock time, and the shift. The shipped rules are generated from the
//! IANA tz database at build time (`build.rs`); this module only
//! evaluates them.

use crate::clock::{civil_from_days, days_from_civil};

/// One edge of the daylight saving period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// 1 to 12.
    pub month: u8,
    /// 0 Sunday to 6 Saturday.
    pub weekday: u8,
    /// 1 to 4, or 5 for the last such weekday of the month.
    pub week: u8,
    /// Local wall clock time, seconds after midnight: standard time at
    /// the start, daylight time at the end.
    pub secs: u32,
}

/// A daylight saving rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DstRule {
    /// When saving starts.
    pub start: Edge,
    /// When it ends.
    pub end: Edge,
    /// Minutes the clock moves, 0 to 120.
    pub shift: u16,
}

const DAY: i64 = 86_400;

impl Edge {
    /// Days since the epoch of the edge's day in `year`.
    fn day(self, year: i64) -> i64 {
        let m = u32::from(self.month.clamp(1, 12));
        if self.week >= 5 {
            let (ny, nm) = if m == 12 {
                (year + 1, 1)
            } else {
                (year, m + 1)
            };
            let last = days_from_civil(ny, nm, 1) - 1;
            last - (weekday(last) - i64::from(self.weekday)).rem_euclid(7)
        } else {
            let first = days_from_civil(year, m, 1);
            first
                + (i64::from(self.weekday) - weekday(first)).rem_euclid(7)
                + (i64::from(self.week.max(1)) - 1) * 7
        }
    }
}

impl DstRule {
    /// The UTC instants (Unix seconds) saving starts and ends in `year`
    /// for a zone `std` seconds east of UTC.
    #[must_use]
    pub fn instants(&self, year: i64, std: i64) -> (i64, i64) {
        let shift = i64::from(self.shift) * 60;
        let start = self.start.day(year) * DAY + i64::from(self.start.secs) - std;
        let end = self.end.day(year) * DAY + i64::from(self.end.secs) - (std + shift);
        (start, end)
    }

    /// Whether saving is in force at `unix` for a zone `std` seconds
    /// east of UTC. A rule that ends in an earlier month than it starts
    /// spans the new year (the southern hemisphere).
    #[must_use]
    pub fn in_force(&self, unix: i64, std: i64) -> bool {
        let year = civil_from_days((unix + std).div_euclid(DAY)).0;
        let (start, end) = self.instants(year, std);
        if self.start.month <= self.end.month {
            unix >= start && unix < end
        } else {
            unix < end || unix >= start
        }
    }
}

/// 0 = Sunday. 1970-01-01 was a Thursday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges() {
        let second_sunday_march = Edge {
            month: 3,
            weekday: 0,
            week: 2,
            secs: 7200,
        };
        assert_eq!(second_sunday_march.day(2026), days_from_civil(2026, 3, 8));
        let last_sunday_october = Edge {
            month: 10,
            weekday: 0,
            week: 5,
            secs: 3600,
        };
        assert_eq!(last_sunday_october.day(2026), days_from_civil(2026, 10, 25));
        let last_monday_march = Edge {
            month: 3,
            weekday: 1,
            week: 5,
            secs: 0,
        };
        assert_eq!(last_monday_march.day(2026), days_from_civil(2026, 3, 30));
        assert_eq!(weekday(days_from_civil(2026, 10, 10)), 6);
    }
}
