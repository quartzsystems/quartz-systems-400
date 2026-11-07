// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*MGTCOL` payload (`docs/PERF.md` "Management collections"): a
//! fixed head, the collection's header, then one frame per interval,
//! appended as the collector records them. Integers in the head and the
//! frame lengths are big-endian; the header and the intervals are in
//! the wire encoding (`work/ipc`).

use ipc::wire_struct;
use thiserror::Error;

use crate::sample::Interval;

/// The head's magic.
pub const MAGIC: &[u8; 8] = b"QSMGTCOL";
/// The payload version.
pub const VERSION: u16 = 1;
/// Offset of the end time in the head.
pub const OFF_ENDED: usize = 12;
/// Offset of the interval count in the head.
pub const OFF_COUNT: usize = 20;
/// The head's length.
pub const HEAD_LEN: usize = 28;

/// What a collection is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// When it started, Unix seconds, local time.
    pub started: u64,
    /// Seconds between samples.
    pub interval: u32,
    /// The profile it was started with.
    pub profile: String,
    /// The categories, as bits (`profile::mask`).
    pub categories: u32,
    /// The system name.
    pub system: String,
    /// The release, `V1R1M0`.
    pub release: String,
    /// The library database files go to as data arrives; blank for
    /// none (`CRTDBF(*NO)`).
    pub dbf_library: String,
    /// Started with `CYCCOL(*YES)`.
    pub cycle: bool,
}

wire_struct!(Header {
    started,
    interval,
    profile,
    categories,
    system,
    release,
    dbf_library,
    cycle
});

/// A collection read back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collection {
    /// Its header.
    pub header: Header,
    /// When it ended; 0 while it is active.
    pub ended: u64,
    /// The intervals, in order.
    pub intervals: Vec<Interval>,
}

/// A payload that does not read.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum MgtcolError {
    /// Not a management collection.
    #[error("not a management collection")]
    Magic,
    /// A version this release does not read.
    #[error("management collection version {0} is not supported")]
    Version(u16),
    /// Cut short or damaged.
    #[error("management collection damaged: {0}")]
    Damaged(String),
}

/// A new collection's payload: the head and the header.
#[must_use]
pub fn new_payload(h: &Header) -> Vec<u8> {
    let hdr = ipc::encode(h);
    let mut out = Vec::with_capacity(HEAD_LEN + hdr.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&u32::try_from(hdr.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(&hdr);
    out
}

/// One interval's frame, to append.
#[must_use]
pub fn frame(i: &Interval) -> Vec<u8> {
    let body = ipc::encode(i);
    let mut out = u32::try_from(body.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(&body);
    out
}

fn be32(b: &[u8], at: usize) -> Result<u32, MgtcolError> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes(s.try_into().expect("four bytes")))
        .ok_or_else(|| MgtcolError::Damaged(format!("short at {at}")))
}

/// Read a payload; frames past the count the head records (a frame
/// written by an interval whose head update was lost) are kept.
pub fn parse(b: &[u8]) -> Result<Collection, MgtcolError> {
    if b.len() < HEAD_LEN || &b[..8] != MAGIC {
        return Err(MgtcolError::Magic);
    }
    let v = u16::from_be_bytes([b[8], b[9]]);
    if v != VERSION {
        return Err(MgtcolError::Version(v));
    }
    let ended = u64::from_be_bytes(b[OFF_ENDED..OFF_ENDED + 8].try_into().expect("eight bytes"));
    let hlen = be32(b, 24)? as usize;
    let hdr = b
        .get(HEAD_LEN..HEAD_LEN + hlen)
        .ok_or_else(|| MgtcolError::Damaged("header".into()))?;
    let header: Header = ipc::decode(hdr).map_err(|e| MgtcolError::Damaged(e.to_string()))?;
    let mut at = HEAD_LEN + hlen;
    let mut intervals = Vec::new();
    while at < b.len() {
        let n = be32(b, at)? as usize;
        let body = b
            .get(at + 4..at + 4 + n)
            .ok_or_else(|| MgtcolError::Damaged(format!("interval at {at}")))?;
        intervals.push(ipc::decode(body).map_err(|e| MgtcolError::Damaged(e.to_string()))?);
        at += 4 + n;
    }
    Ok(Collection {
        header,
        ended,
        intervals,
    })
}

/// Day of the year (1-based), hours, minutes and seconds of a Unix time.
#[must_use]
pub fn day_and_time(secs: u64) -> (u32, u32, u32, u32) {
    let (y, _, _) = civil(secs / 86_400);
    let jan1 = days_from_civil(y, 1, 1);
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let doy = u32::try_from(days - jan1 + 1).unwrap_or(1);
    let s = u32::try_from(secs % 86_400).unwrap_or(0);
    (doy, s / 3600, s / 60 % 60, s % 60)
}

/// A collection's name: `Q`, the day of the year and the time it
/// started (`Q303055930`).
#[must_use]
pub fn name_of(local: u64) -> String {
    let (d, h, m, s) = day_and_time(local);
    format!("Q{d:03}{h:02}{m:02}{s:02}")
}

/// Year, month and day of a day count since 1970.
#[must_use]
pub fn civil(days: u64) -> (i64, u32, u32) {
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::JobData;

    #[test]
    fn payload_round_trip() {
        let h = Header {
            started: 1_000,
            interval: 15,
            profile: "*STANDARD".into(),
            categories: 0x7F,
            system: "QS400".into(),
            release: "V1R1M0".into(),
            dbf_library: "QPFRDATA".into(),
            cycle: false,
        };
        let mut p = new_payload(&h);
        let i = Interval {
            number: 1,
            end: 1_015,
            secs: 15,
            jobs: vec![JobData {
                name: "QPADEV0001".into(),
                cpu_ms: 40,
                ..JobData::default()
            }],
            ..Interval::default()
        };
        p.extend(frame(&i));
        p.extend(frame(&Interval {
            number: 2,
            ..i.clone()
        }));
        p[OFF_ENDED..OFF_ENDED + 8].copy_from_slice(&1_030u64.to_be_bytes());
        let c = parse(&p).unwrap();
        assert_eq!(c.header, h);
        assert_eq!(c.ended, 1_030);
        assert_eq!(c.intervals.len(), 2);
        assert_eq!(c.intervals[0], i);
        assert_eq!(parse(b"nope").unwrap_err(), MgtcolError::Magic);
        p.truncate(p.len() - 3);
        assert!(matches!(parse(&p), Err(MgtcolError::Damaged(_))));
    }

    #[test]
    fn names() {
        // 2026-10-30 05:59:30 UTC is day 303.
        assert_eq!(name_of(1_793_339_970), "Q303055930");
        assert_eq!(name_of(0), "Q001000000");
        assert_eq!(civil(0), (1970, 1, 1));
    }
}
