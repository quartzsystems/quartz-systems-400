// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Time zone descriptions (`*TIMZON`) for `QTIMZON`. The shipped set is
//! IBM's (`zones.toml`, from the primary's `WRKTIMZON`), with daylight
//! saving rules generated at build time from the IANA tz database
//! vendored in `tzdata/`. The storage service holds the descriptions as
//! objects in QSYS and [`install`]s the set it finds there, so a zone
//! made by `CRTTIMZON` or changed by `CHGTIMZON` is the one [`find`]
//! answers with; a process that installed nothing sees the shipped set.

mod rules;

use std::sync::{LazyLock, RwLock};

pub use rules::{DstRule, Edge};

/// Where a shipped zone's rule came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The tz database's rule of the alternate zone in that year.
    Tzdata(i64),
    /// IBM's variant, which no tzdata year gives.
    Ibm,
}

/// A shipped rule as the build wrote it.
#[derive(Clone, Copy, Debug)]
pub struct ShippedRule {
    /// Start.
    pub start: Edge,
    /// End.
    pub end: Edge,
    /// Minutes.
    pub shift: u16,
    /// Where it came from.
    pub source: Source,
}

/// A shipped zone as the build wrote it.
#[derive(Clone, Copy, Debug)]
pub struct Shipped {
    /// The object name.
    pub name: &'static str,
    /// Minutes east of UTC.
    pub offset: i32,
    /// Standard time: abbreviation, message, full name.
    pub std: (&'static str, &'static str, &'static str),
    /// Daylight time, when the zone has it.
    pub dst: Option<(&'static str, &'static str, &'static str)>,
    /// The alternate (IANA) name.
    pub alt: &'static str,
    /// The rule.
    pub rule: Option<ShippedRule>,
}

include!(concat!(env!("OUT_DIR"), "/zones.rs"));

/// One time zone description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    /// The object name (the `QTIMZON` value).
    pub name: String,
    /// Standard offset from UTC in minutes, -779 to 779.
    pub offset: i32,
    /// Standard time abbreviation.
    pub std_abbr: String,
    /// Standard time full name.
    pub std_full: String,
    /// The message the standard names come from (`*MSG`); empty when
    /// they were given.
    pub std_msg: String,
    /// Daylight time abbreviation; empty is `*NONE`.
    pub dst_abbr: String,
    /// Daylight time full name.
    pub dst_full: String,
    /// The message the daylight names come from.
    pub dst_msg: String,
    /// The message file of the messages: name and library.
    pub msgf: (String, String),
    /// The daylight saving rule.
    pub dst: Option<DstRule>,
    /// Years added to the Gregorian year (`YEAROFS`).
    pub year_offset: i16,
    /// The alternate name (an IANA zone).
    pub alt: String,
    /// `TEXT`.
    pub text: String,
}

/// The shipped set, built once.
static SHIPPED_ZONES: LazyLock<Vec<Zone>> =
    LazyLock::new(|| SHIPPED.iter().map(Zone::from_shipped).collect());

/// The set the storage service installed.
static LIVE: RwLock<Option<Vec<Zone>>> = RwLock::new(None);

/// The zones QS/400 ships.
#[must_use]
pub fn shipped() -> &'static [Zone] {
    &SHIPPED_ZONES
}

/// The message text of a zone message (`CPX0910`).
#[must_use]
pub fn message_text(id: &str) -> Option<&'static str> {
    MESSAGES.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// Make `zones` the set [`find`] and [`all`] answer from.
pub fn install(zones: Vec<Zone>) {
    *LIVE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(zones);
}

/// Every zone: the installed set, else the shipped one.
#[must_use]
pub fn all() -> Vec<Zone> {
    LIVE.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| shipped().to_vec())
}

/// The `QTIMZON` value this process last saw ([`crate::SysVals`] sets it).
static CURRENT: RwLock<String> = RwLock::new(String::new());

/// Note the `QTIMZON` value.
pub fn set_current(name: &str) {
    *CURRENT
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = name.trim().to_string();
}

/// The offset from UTC in seconds at `unix` of the zone `QTIMZON`
/// names (UTC before any value was seen or for an unknown name).
#[must_use]
pub fn current_offset(unix: u64) -> i64 {
    let name = CURRENT
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    find(&name).map_or(0, |z| i64::from(z.offset_at(unix)))
}

/// A zone by name.
#[must_use]
pub fn find(name: &str) -> Option<Zone> {
    let name = name.trim();
    let live = LIVE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match live.as_ref() {
        Some(z) => z.iter().find(|z| z.name == name).cloned(),
        None => shipped().iter().find(|z| z.name == name).cloned(),
    }
}

/// The `QTIMZON` value of V1R0M0 a name stood for and the shipped zone
/// it becomes: V1R0M0 had its own table whose names IBM's set reuses
/// with other rules (its `QN0500EST2` is IBM's `QN0500EST3`).
#[must_use]
pub fn v1r0m0_name(old: &str) -> Option<&'static str> {
    const MAP: &[(&str, &str)] = &[
        ("QN1000HST", "QN1000HST"),
        ("QN0900AKST2", "QN0900AST2"),
        ("QN0800PST", "QN0800UTCS"),
        ("QN0800PST2", "QN0800PST2"),
        ("QN0700MST", "QN0700MST2"),
        ("QN0700MST2", "QN0700MST3"),
        ("QN0600CST", "QN0600UTCS"),
        ("QN0600CST2", "QN0600CST2"),
        ("QN0500EST", "QN0500EST2"),
        ("QN0500EST2", "QN0500EST3"),
        ("QN0400AST", "QN0400UTCS"),
        ("QN0400AST2", "QN0400AST2"),
        ("QN0330NST2", "QN0330NST3"),
        ("QN0300BRT", "QN0300UTCS"),
        ("QN0000UTC", "Q0000UTC"),
        ("QN0000GMT", "Q0000GMT"),
        ("QN0000GMT2", "Q0000GMT3"),
        ("QP0100CET", "QP0100UTCS"),
        ("QP0100CET2", "QP0100CET4"),
        ("QP0200EET", "QP0200EET2"),
        ("QP0200EET2", "QP0200EET3"),
        ("QP0300MSK", "QP0300UTCS"),
        ("QP0530IST", "QP0530IST"),
        ("QP0800CST", "QP0800BST"),
        ("QP0900JST", "QP0900JST"),
        ("QP1000AEST", "QP1000UTCS"),
        ("QP1000AEST2", "QP1000AES2"),
        ("QP1200NZST2", "QP1200NZS3"),
    ];
    MAP.iter().find(|(o, _)| *o == old.trim()).map(|(_, n)| *n)
}

/// The order `WRKTIMZON` lists zones in: UTC's offset first, then the
/// zones west of it nearest first, then those east of it farthest
/// first; by name (EBCDIC) within an offset.
#[must_use]
pub fn list_order(a: &Zone, b: &Zone) -> std::cmp::Ordering {
    let key = |z: &Zone| match z.offset.signum() {
        0 => (0, 0),
        -1 => (1, -z.offset),
        _ => (2, -z.offset),
    };
    key(a)
        .cmp(&key(b))
        .then_with(|| ebcdic(&a.name).cmp(&ebcdic(&b.name)))
}

fn ebcdic(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| ccsid::byte_of(c).unwrap_or(0xFF))
        .collect()
}

const H: i32 = 3600;

impl Zone {
    fn from_shipped(s: &Shipped) -> Self {
        let (dst_abbr, dst_msg, dst_full) = s.dst.unwrap_or(("", "", ""));
        Self {
            name: s.name.into(),
            offset: s.offset,
            std_abbr: s.std.0.into(),
            std_full: s.std.2.into(),
            std_msg: s.std.1.into(),
            dst_abbr: dst_abbr.into(),
            dst_full: dst_full.into(),
            dst_msg: dst_msg.into(),
            msgf: ("QCPFMSG".into(), "*LIBL".into()),
            dst: s.rule.map(|r| DstRule {
                start: r.start,
                end: r.end,
                shift: r.shift,
            }),
            year_offset: 0,
            alt: s.alt.into(),
            text: String::new(),
        }
    }

    /// The standard offset in seconds.
    #[must_use]
    pub fn std_offset(&self) -> i32 {
        self.offset * 60
    }

    /// Whether daylight saving is in force at `unix`.
    #[must_use]
    pub fn in_dst(&self, unix: u64) -> bool {
        let unix = i64::try_from(unix).unwrap_or(i64::MAX);
        self.dst
            .is_some_and(|r| r.in_force(unix, i64::from(self.std_offset())))
    }

    /// Offset from UTC in seconds at `unix`, daylight saving included.
    #[must_use]
    pub fn offset_at(&self, unix: u64) -> i32 {
        match self.dst {
            Some(r) if self.in_dst(unix) => self.std_offset() + i32::from(r.shift) * 60,
            _ => self.std_offset(),
        }
    }

    /// The offset as `sHHMM`, the `QUTCOFFSET` form.
    #[must_use]
    pub fn utc_offset_text(&self, unix: u64) -> String {
        let off = self.offset_at(unix);
        let sign = if off < 0 { '-' } else { '+' };
        let a = off.abs();
        format!("{sign}{:02}{:02}", a / H, (a % H) / 60)
    }

    /// The abbreviation in force at `unix`.
    #[must_use]
    pub fn abbr_at(&self, unix: u64) -> &str {
        if self.in_dst(unix) {
            &self.dst_abbr
        } else {
            &self.std_abbr
        }
    }

    /// The description a list shows: the text, else the standard name.
    #[must_use]
    pub fn description(&self) -> &str {
        if self.text.trim().is_empty() {
            &self.std_full
        } else {
            &self.text
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::civil_to_unix;

    #[test]
    fn the_shipped_set_is_ibms() {
        assert_eq!(shipped().len(), 93);
        let mut names: Vec<&str> = shipped().iter().map(|z| z.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 93);
        assert!(find("Q0000UTC").is_some_and(|z| z.dst.is_none() && z.offset == 0));
        assert!(
            SHIPPED
                .iter()
                .filter(|z| matches!(
                    z.rule,
                    Some(ShippedRule {
                        source: Source::Tzdata(_),
                        ..
                    })
                ))
                .count()
                >= 35
        );
        assert!(!TZDATA_VERSION.is_empty());
    }

    #[test]
    fn eastern_offsets_across_the_year() {
        let est = find("QN0500EST3").unwrap();
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
        assert_eq!(est.abbr_at(civil_to_unix(2026, 7, 4, 12, 0, 0)), "EDT");
        // The pre-2007 rule: the first Sunday of April.
        let old = find("QN0500EST").unwrap();
        assert_eq!(old.offset_at(civil_to_unix(2026, 3, 20, 12, 0, 0)), -5 * H);
        assert_eq!(old.offset_at(civil_to_unix(2026, 4, 6, 12, 0, 0)), -4 * H);
        let fixed = find("QN0500EST2").unwrap();
        assert_eq!(fixed.offset_at(civil_to_unix(2026, 7, 4, 12, 0, 0)), -5 * H);
    }

    #[test]
    fn european_offsets() {
        let cet = find("QP0100CET4").unwrap();
        // 2026: last Sunday of March is the 29th, of October the 25th; 01:00 UTC.
        assert_eq!(cet.offset_at(civil_to_unix(2026, 3, 29, 0, 59, 59)), H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 3, 29, 1, 0, 0)), 2 * H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 10, 25, 0, 59, 59)), 2 * H);
        assert_eq!(cet.offset_at(civil_to_unix(2026, 10, 25, 1, 0, 0)), H);
    }

    #[test]
    fn southern_hemisphere_spans_new_year() {
        let nz = find("QP1200NZS3").unwrap();
        assert_eq!(nz.offset_at(civil_to_unix(2026, 1, 10, 0, 0, 0)), 13 * H);
        assert_eq!(nz.offset_at(civil_to_unix(2026, 6, 10, 0, 0, 0)), 12 * H);
        assert_eq!(nz.offset_at(civil_to_unix(2026, 12, 10, 0, 0, 0)), 13 * H);
    }

    #[test]
    fn installed_zones_answer() {
        let mut z = find("Q0000UTC").unwrap();
        z.name = "QSTEST".into();
        z.offset = 90;
        let mut all = shipped().to_vec();
        all.push(z);
        install(all);
        assert_eq!(find("QSTEST").unwrap().utc_offset_text(0), "+0130");
        install(shipped().to_vec());
        assert!(find("QSTEST").is_none());
    }

    #[test]
    fn list_order_is_wrktimzons() {
        let mut v = shipped().to_vec();
        v.sort_by(list_order);
        let names: Vec<&str> = v.iter().map(|z| z.name.as_str()).collect();
        let want: Vec<&str> = SHIPPED.iter().map(|z| z.name).collect();
        assert_eq!(names, want);
    }

    #[test]
    fn v1r0m0_names_map_to_shipped_zones() {
        for old in ["QN0500EST2", "QN0000UTC", "QP1200NZST2", "QP0100CET2"] {
            assert!(find(v1r0m0_name(old).unwrap()).is_some(), "{old}");
        }
        assert_eq!(v1r0m0_name("QN0500EST2"), Some("QN0500EST3"));
    }
}
