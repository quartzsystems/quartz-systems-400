// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Time zone descriptions for QTIMZON. A fixed table with the daylight
//! saving rules coded here; nothing comes from a tz database.
//!
//! Naming follows the IBM convention: `QN`/`QP` for a negative/positive
//! offset, four digits `HHMM`, an abbreviation, and a trailing `2` when
//! daylight saving time applies.

mod rules;

pub use rules::{DstRule, in_dst};

/// One zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    /// QTIMZON value.
    pub name: &'static str,
    /// Standard offset from UTC in seconds.
    pub std_offset: i32,
    /// Daylight saving rule, if the zone observes it.
    pub dst: Option<DstRule>,
    /// Quartz description.
    pub text: &'static str,
}

const H: i32 = 3600;
const M: i32 = 60;

/// Every zone QS/400 knows.
pub const ZONES: &[Zone] = &[
    Zone {
        name: "QN1000HST",
        std_offset: -10 * H,
        dst: None,
        text: "Hawaii standard time",
    },
    Zone {
        name: "QN0900AKST2",
        std_offset: -9 * H,
        dst: Some(DstRule::Us),
        text: "Alaska, US daylight rules",
    },
    Zone {
        name: "QN0800PST",
        std_offset: -8 * H,
        dst: None,
        text: "Pacific standard time",
    },
    Zone {
        name: "QN0800PST2",
        std_offset: -8 * H,
        dst: Some(DstRule::Us),
        text: "Pacific, US daylight rules",
    },
    Zone {
        name: "QN0700MST",
        std_offset: -7 * H,
        dst: None,
        text: "Mountain standard time",
    },
    Zone {
        name: "QN0700MST2",
        std_offset: -7 * H,
        dst: Some(DstRule::Us),
        text: "Mountain, US daylight rules",
    },
    Zone {
        name: "QN0600CST",
        std_offset: -6 * H,
        dst: None,
        text: "Central standard time",
    },
    Zone {
        name: "QN0600CST2",
        std_offset: -6 * H,
        dst: Some(DstRule::Us),
        text: "Central, US daylight rules",
    },
    Zone {
        name: "QN0500EST",
        std_offset: -5 * H,
        dst: None,
        text: "Eastern standard time",
    },
    Zone {
        name: "QN0500EST2",
        std_offset: -5 * H,
        dst: Some(DstRule::Us),
        text: "Eastern, US daylight rules",
    },
    Zone {
        name: "QN0400AST",
        std_offset: -4 * H,
        dst: None,
        text: "Atlantic standard time",
    },
    Zone {
        name: "QN0400AST2",
        std_offset: -4 * H,
        dst: Some(DstRule::Us),
        text: "Atlantic, US daylight rules",
    },
    Zone {
        name: "QN0330NST2",
        std_offset: -3 * H - 30 * M,
        dst: Some(DstRule::Us),
        text: "Newfoundland, US daylight rules",
    },
    Zone {
        name: "QN0300BRT",
        std_offset: -3 * H,
        dst: None,
        text: "Brasilia time",
    },
    Zone {
        name: "QN0000UTC",
        std_offset: 0,
        dst: None,
        text: "Coordinated universal time",
    },
    Zone {
        name: "QN0000GMT",
        std_offset: 0,
        dst: None,
        text: "Greenwich mean time, no daylight saving",
    },
    Zone {
        name: "QN0000GMT2",
        std_offset: 0,
        dst: Some(DstRule::Eu),
        text: "United Kingdom and Ireland, EU daylight rules",
    },
    Zone {
        name: "QP0100CET",
        std_offset: H,
        dst: None,
        text: "Central European standard time",
    },
    Zone {
        name: "QP0100CET2",
        std_offset: H,
        dst: Some(DstRule::Eu),
        text: "Central Europe, EU daylight rules",
    },
    Zone {
        name: "QP0200EET",
        std_offset: 2 * H,
        dst: None,
        text: "Eastern European standard time",
    },
    Zone {
        name: "QP0200EET2",
        std_offset: 2 * H,
        dst: Some(DstRule::Eu),
        text: "Eastern Europe, EU daylight rules",
    },
    Zone {
        name: "QP0300MSK",
        std_offset: 3 * H,
        dst: None,
        text: "Moscow time",
    },
    Zone {
        name: "QP0530IST",
        std_offset: 5 * H + 30 * M,
        dst: None,
        text: "India standard time",
    },
    Zone {
        name: "QP0800CST",
        std_offset: 8 * H,
        dst: None,
        text: "China standard time",
    },
    Zone {
        name: "QP0900JST",
        std_offset: 9 * H,
        dst: None,
        text: "Japan standard time",
    },
    Zone {
        name: "QP1000AEST",
        std_offset: 10 * H,
        dst: None,
        text: "Australian eastern standard time",
    },
    Zone {
        name: "QP1000AEST2",
        std_offset: 10 * H,
        dst: Some(DstRule::AuSouth),
        text: "Australian eastern, southern daylight rules",
    },
    Zone {
        name: "QP1200NZST2",
        std_offset: 12 * H,
        dst: Some(DstRule::Nz),
        text: "New Zealand, NZ daylight rules",
    },
];

/// Find a zone by QTIMZON value.
pub fn find(name: &str) -> Option<&'static Zone> {
    ZONES.iter().find(|z| z.name == name)
}

impl Zone {
    /// Offset from UTC in seconds at `unix`, daylight saving included.
    pub fn offset_at(&self, unix: u64) -> i32 {
        match self.dst {
            Some(rule) if in_dst(rule, unix, self.std_offset) => self.std_offset + H,
            _ => self.std_offset,
        }
    }

    /// The offset as `sHHMM`, the QUTCOFFSET form.
    pub fn utc_offset_text(&self, unix: u64) -> String {
        let off = self.offset_at(unix);
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.abs();
        format!("{sign}{:02}{:02}", a / H, (a % H) / M)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::civil_to_unix;

    #[test]
    fn table_has_enough_zones_with_rules() {
        assert!(ZONES.len() >= 12);
        assert!(ZONES.iter().filter(|z| z.dst.is_some()).count() >= 6);
        let mut names: Vec<&str> = ZONES.iter().map(|z| z.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ZONES.len());
    }

    #[test]
    fn eastern_offsets_across_the_year() {
        let est = find("QN0500EST2").unwrap();
        assert_eq!(
            est.utc_offset_text(civil_to_unix(2026, 1, 15, 12, 0, 0)),
            "-0500"
        );
        assert_eq!(
            est.utc_offset_text(civil_to_unix(2026, 7, 4, 12, 0, 0)),
            "-0400"
        );
        // 2026: DST starts March 8 at 07:00 UTC, ends November 1 at 06:00 UTC.
        assert_eq!(est.offset_at(civil_to_unix(2026, 3, 8, 6, 59, 59)), -5 * H);
        assert_eq!(est.offset_at(civil_to_unix(2026, 3, 8, 7, 0, 0)), -4 * H);
        assert_eq!(est.offset_at(civil_to_unix(2026, 11, 1, 5, 59, 59)), -4 * H);
        assert_eq!(est.offset_at(civil_to_unix(2026, 11, 1, 6, 0, 0)), -5 * H);
        let fixed = find("QN0500EST").unwrap();
        assert_eq!(fixed.offset_at(civil_to_unix(2026, 7, 4, 12, 0, 0)), -5 * H);
    }

    #[test]
    fn european_offsets() {
        let cet = find("QP0100CET2").unwrap();
        // 2026: last Sunday of March is the 29th, of October the 25th; 01:00 UTC.
        assert_eq!(cet.offset_at(civil_to_unix(2026, 3, 29, 0, 59, 59)), H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 3, 29, 1, 0, 0)), 2 * H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 10, 25, 0, 59, 59)), 2 * H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 10, 25, 1, 0, 0)), H);
    }

    #[test]
    fn southern_hemisphere_spans_new_year() {
        let syd = find("QP1000AEST2").unwrap();
        assert_eq!(syd.offset_at(civil_to_unix(2026, 1, 10, 0, 0, 0)), 11 * H);
        assert_eq!(syd.offset_at(civil_to_unix(2026, 6, 10, 0, 0, 0)), 10 * H);
        assert_eq!(syd.offset_at(civil_to_unix(2026, 12, 10, 0, 0, 0)), 11 * H);
    }
}
