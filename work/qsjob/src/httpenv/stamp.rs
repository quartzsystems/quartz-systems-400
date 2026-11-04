// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A moment as a job shows it: the date in its `DATFMT` with its
//! `DATSEP`, the time with its `TIMSEP`, local to the system's time
//! zone (`QTIMZON`).

/// `secs` (UTC) at `offset` seconds east of UTC, as `(date, time)`, in
/// the formats the values name (`ccsid::nls::Formats`, the one path).
pub fn stamp(secs: u64, offset: i32, datfmt: &str, datsep: &str, timsep: &str) -> (String, String) {
    ccsid::nls::Formats::parse(datfmt, datsep, timsep, "").stamp(i64::try_from(secs).unwrap_or(0), i64::from(offset))
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
