// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The trigger buffer a trigger program gets, byte for byte as the
//! primary builds it (`docs/DB_FORMAT.md` §8.3 `TriggerBuffer`,
//! `tools/conformance/sql/oracle/trigger/primary-facts.md`): a 96-byte
//! header, the old record and its null map, the new record and its null
//! map, each record at a 16-byte boundary 16 bytes past the null map
//! before it. Null maps have one byte per field, `0` or `1`.

use qsname::Name;

use super::model::{Event, Time};
use crate::format::FormatDef;
use crate::record;
use crate::spec::{Layout, field};

/// Bytes of the header.
pub const HEADER_LEN: usize = 96;

/// The header layout.
pub static LAYOUT: Layout = Layout {
    name: "TriggerBuffer",
    size: Some(HEADER_LEN),
    fields: &[
        field("file", 0, 10),
        field("library", 10, 10),
        field("member", 20, 10),
        field("event", 30, 1),
        field("time", 31, 1),
        field("commit_level", 32, 1),
        field("reserved", 33, 3),
        field("ccsid", 36, 4),
        field("rrn", 40, 4),
        field("reserved2", 44, 4),
        field("old_offset", 48, 4),
        field("old_len", 52, 4),
        field("old_null_offset", 56, 4),
        field("old_null_len", 60, 4),
        field("new_offset", 64, 4),
        field("new_len", 68, 4),
        field("new_null_offset", 72, 4),
        field("new_null_len", 76, 4),
        field("reserved3", 80, 16),
    ],
};

/// A record image: its bytes and the member's null map.
pub type Image<'a> = (&'a [u8], &'a [u8]);

/// Everything the header says.
#[derive(Clone, Copy, Debug)]
pub struct Header {
    /// The file.
    pub file: Name,
    /// Its library.
    pub library: Name,
    /// The member.
    pub member: Name,
    /// The event.
    pub event: Event,
    /// The time.
    pub time: Time,
    /// The commit lock level digit: `0` none, `1` `*CHG`, `2` `*CS`, `3` `*ALL`.
    pub commit_level: u8,
    /// The data's CCSID.
    pub ccsid: u32,
    /// The record's number, 0 for a before insert.
    pub rrn: u32,
}

/// Where a buffer's parts are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parts {
    /// Record length.
    pub rec_len: usize,
    /// Fields: null map bytes.
    pub fields: usize,
    /// The new record's offset.
    pub new: usize,
    /// The buffer's length.
    pub total: usize,
}

const fn past(end: usize) -> usize {
    end.div_ceil(16) * 16 + 16
}

impl Parts {
    /// For a format.
    #[must_use]
    pub fn of(rec_len: usize, fields: usize) -> Self {
        let new = past(HEADER_LEN + rec_len + fields);
        Self { rec_len, fields, new, total: past(new + rec_len + fields) }
    }
}

fn put32(b: &mut [u8], at: usize, v: usize) {
    b[at..at + 4].copy_from_slice(&u32::try_from(v).expect("fits").to_be_bytes());
}

/// The per-field null map of an image.
fn null_bytes(fmt: &FormatDef, nulls: &[u8]) -> Vec<u8> {
    (0..fmt.format.fields.len())
        .map(|i| if record::is_null(nulls, fmt.null_index(i)) { 0xF1 } else { 0xF0 })
        .collect()
}

/// Build the buffer for `fmt`'s records.
#[must_use]
pub fn build(h: &Header, fmt: &FormatDef, old: Option<Image<'_>>, new: Option<Image<'_>>) -> Vec<u8> {
    let rec_len = fmt.format.record_len as usize;
    let fields = fmt.format.fields.len();
    let p = Parts::of(rec_len, fields);
    let mut b = vec![0x40u8; p.total];
    b[..HEADER_LEN].fill(0);
    b[0..10].copy_from_slice(h.file.as_bytes());
    b[10..20].copy_from_slice(h.library.as_bytes());
    b[20..30].copy_from_slice(h.member.as_bytes());
    b[30] = 0xF0 + h.event.code();
    b[31] = if h.time.buffer_code() == b'1' { 0xF1 } else { 0xF2 };
    b[32] = 0xF0 + h.commit_level;
    put32(&mut b, 36, h.ccsid as usize);
    put32(&mut b, 40, h.rrn as usize);
    let old_null = HEADER_LEN + rec_len;
    put32(&mut b, 48, HEADER_LEN);
    put32(&mut b, 52, rec_len);
    put32(&mut b, 56, old_null);
    put32(&mut b, 60, fields);
    put32(&mut b, 64, p.new);
    put32(&mut b, 68, rec_len);
    put32(&mut b, 72, p.new + rec_len);
    put32(&mut b, 76, fields);
    let mut place = |at: usize, img: Option<Image<'_>>| {
        if let Some((rec, nulls)) = img {
            let n = rec.len().min(rec_len);
            b[at..at + n].copy_from_slice(&rec[..n]);
            b[at + rec_len..at + rec_len + fields].copy_from_slice(&null_bytes(fmt, nulls));
        }
    };
    place(HEADER_LEN, old);
    place(p.new, new);
    b
}

/// The new record and the member's null map back from a buffer the
/// program may have changed (`ALWREPCHG(*YES)`).
#[must_use]
pub fn new_image(fmt: &FormatDef, buf: &[u8], nulls_len: usize) -> Option<(Vec<u8>, Vec<u8>)> {
    let p = Parts::of(fmt.format.record_len as usize, fmt.format.fields.len());
    if buf.len() < p.new + p.rec_len + p.fields {
        return None;
    }
    let rec = buf[p.new..p.new + p.rec_len].to_vec();
    let mut nulls = vec![0u8; nulls_len];
    for (i, &c) in buf[p.new + p.rec_len..p.new + p.rec_len + p.fields].iter().enumerate() {
        if let Some(k) = fmt.null_index(i) {
            record::set_null(&mut nulls, k, c == 0xF1);
        }
    }
    Some((rec, nulls))
}

/// Put another new image into a buffer, as the next trigger of the same
/// change sees what the one before it left.
pub fn replace_new(fmt: &FormatDef, buf: &mut [u8], new: Image<'_>) {
    let p = Parts::of(fmt.format.record_len as usize, fmt.format.fields.len());
    if buf.len() < p.new + p.rec_len + p.fields {
        return;
    }
    let n = new.0.len().min(p.rec_len);
    buf[p.new..p.new + n].copy_from_slice(&new.0[..n]);
    buf[p.new + p.rec_len..p.new + p.rec_len + p.fields].copy_from_slice(&null_bytes(fmt, new.1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_as_the_primary_lays_them_out() {
        // Record length and fields of the probes' tables, and what came back.
        for (len, fields, new, total) in [(22, 4, 144, 192), (104, 2, 224, 352), (20, 20, 160, 224), (5, 1, 128, 160)] {
            let p = Parts::of(len, fields);
            assert_eq!((p.new, p.total), (new, total), "{len} {fields}");
        }
    }
}
