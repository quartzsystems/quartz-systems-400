// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The feedback areas. `S38OPNFB` (98 bytes): open flags at 0 (`0x80`
//! and `0x40` input, `0x20` output), file, library and member at 1, 11
//! and 21, the record length at 31, input and output buffer lengths at
//! 35 and 39, the record count at 43, the access type (`AR` arrival,
//! `KU`/`KF` keyed) at 47, the duplicate key flags at 49, the most
//! records per transfer at 61, the record increment at 63 and the CCSID
//! at 88. `S38IOFB` (59 bytes): the record format at 0, the record
//! length at 10, the records processed at 14 and the last relative
//! record number at 55.

// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

use crate::Error;
use crate::cp::param;
use crate::wire::{Object, ebcdic_fixed, text};

/// The length of `S38OPNFB`.
pub const OPNFB_LEN: usize = 98;
/// The length of `S38IOFB`.
pub const IOFB_LEN: usize = 59;

/// `S38OPNFB` fields: offset, size, name.
pub const OPNFB_LAYOUT: &[(usize, usize, &str)] = &[
    (0, 1, "open_flags"),
    (1, 10, "file"),
    (11, 10, "library"),
    (21, 10, "member"),
    (31, 2, "record_length"),
    (35, 4, "input_buffer_length"),
    (39, 4, "output_buffer_length"),
    (43, 4, "records"),
    (47, 2, "access_type"),
    (49, 1, "duplicate_keys"),
    (61, 2, "records_per_transfer"),
    (63, 2, "record_increment"),
    (88, 2, "ccsid"),
];

/// `S38IOFB` fields: offset, size, name.
pub const IOFB_LAYOUT: &[(usize, usize, &str)] = &[
    (0, 10, "record_format"),
    (10, 4, "record_length"),
    (14, 2, "records_processed"),
    (55, 4, "relative_record_number"),
];

/// The offset of a field of a layout.
fn at(layout: &[(usize, usize, &str)], name: &str) -> usize {
    layout.iter().find(|f| f.2 == name).map_or(0, |f| f.0)
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_be_bytes());
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The open feedback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenFeedback {
    /// Opened for input.
    pub input: bool,
    /// Opened for output.
    pub output: bool,
    /// The file.
    pub file: String,
    /// The library.
    pub library: String,
    /// The member.
    pub member: String,
    /// The record length.
    pub reclen: u16,
    /// The records in the member.
    pub records: u32,
    /// Keyed access.
    pub keyed: bool,
    /// Records per transfer.
    pub blocking: u16,
    /// The CCSID of the file's character data.
    pub ccsid: u16,
}

impl OpenFeedback {
    /// The reply object.
    pub fn to_object(&self) -> Object {
        let mut b = vec![0u8; OPNFB_LEN];
        b[0] = 0x80 | if self.input { 0x40 } else { 0 } | if self.output { 0x20 } else { 0 };
        let o = |n| at(OPNFB_LAYOUT, n);
        let f = o("file");
        b[f..f + 10].copy_from_slice(&ebcdic_fixed(&self.file, 10));
        let l = o("library");
        b[l..l + 10].copy_from_slice(&ebcdic_fixed(&self.library, 10));
        let m = o("member");
        b[m..m + 10].copy_from_slice(&ebcdic_fixed(&self.member, 10));
        put16(&mut b, o("record_length"), self.reclen);
        let buflen = u32::from(self.reclen) * u32::from(self.blocking.max(1));
        put32(&mut b, o("input_buffer_length"), buflen);
        put32(&mut b, o("output_buffer_length"), buflen);
        put32(&mut b, o("records"), self.records);
        let a = o("access_type");
        b[a..a + 2].copy_from_slice(&ebcdic_fixed(if self.keyed { "KU" } else { "AR" }, 2));
        put16(&mut b, o("records_per_transfer"), self.blocking.max(1));
        put16(&mut b, o("record_increment"), 1);
        put16(&mut b, o("ccsid"), self.ccsid);
        Object::data(param::S38OPNFB, b)
    }

    /// From the reply object's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < OPNFB_LEN.min(90) {
            return Err(Error::Short {
                what: "S38OPNFB",
                need: 90,
                have: b.len(),
            });
        }
        Ok(Self {
            input: b[0] & 0x40 != 0,
            output: b[0] & 0x20 != 0,
            file: text(&b[1..11]),
            library: text(&b[11..21]),
            member: text(&b[21..31]),
            reclen: get16(b, at(OPNFB_LAYOUT, "record_length")),
            records: get32(b, at(OPNFB_LAYOUT, "records")),
            keyed: b[at(OPNFB_LAYOUT, "access_type")] == crate::wire::ebcdic("K")[0],
            blocking: get16(b, at(OPNFB_LAYOUT, "records_per_transfer")),
            ccsid: get16(b, at(OPNFB_LAYOUT, "ccsid")),
        })
    }
}

/// The I/O feedback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Iofb {
    /// The record format.
    pub format: String,
    /// The record length.
    pub reclen: u32,
    /// Records processed.
    pub count: u16,
    /// The last relative record number.
    pub rrn: u32,
}

impl Iofb {
    /// The reply object.
    pub fn to_object(&self) -> Object {
        let mut b = vec![0u8; IOFB_LEN];
        b[0..10].copy_from_slice(&ebcdic_fixed(&self.format, 10));
        put32(&mut b, at(IOFB_LAYOUT, "record_length"), self.reclen);
        put16(&mut b, at(IOFB_LAYOUT, "records_processed"), self.count);
        put32(&mut b, at(IOFB_LAYOUT, "relative_record_number"), self.rrn);
        Object::data(param::S38IOFB, b)
    }

    /// From the reply object's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < IOFB_LEN {
            return Err(Error::Short {
                what: "S38IOFB",
                need: IOFB_LEN,
                have: b.len(),
            });
        }
        Ok(Self {
            format: text(&b[0..10]),
            reclen: get32(b, at(IOFB_LAYOUT, "record_length")),
            count: get16(b, at(IOFB_LAYOUT, "records_processed")),
            rrn: get32(b, at(IOFB_LAYOUT, "relative_record_number")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let f = OpenFeedback {
            input: true,
            output: true,
            file: "CUST".into(),
            library: "QSTEST".into(),
            member: "CUST".into(),
            reclen: 40,
            records: 3,
            keyed: true,
            blocking: 1,
            ccsid: 37,
        };
        assert_eq!(OpenFeedback::decode(f.to_object().bytes()).unwrap(), f);
        let i = Iofb {
            format: "CUSTR".into(),
            reclen: 40,
            count: 1,
            rrn: 9,
        };
        assert_eq!(Iofb::decode(i.to_object().bytes()).unwrap(), i);
    }
}
