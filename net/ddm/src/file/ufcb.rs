// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The user file control block of `S38OPEN`. Fixed part: the file (10),
//! `0x0048` and the library (10), `0x0049` and the member (10), twelve
//! reserved bytes, the open flags (two bytes at 46: `0x20` input,
//! `0x10` output, `0x08` update, `0x04` delete), the release `0100` in
//! EBCDIC, reserved bytes to 83 (80 holds `0x0006`). Then entries ending in `0x7FFF`: `0x3C`
//! with `0x80` for arrival sequence, `0x3A` with the share options, the
//! blocking factor, and `0x0009` with the record format list. QS/400
//! reads the names, the flags, the access order, the blocking factor
//! and the format, and writes the layout `JTOpen` and IBM i send.

use crate::Error;
use crate::wire::{ebcdic_fixed, text};

/// Open flag: read.
pub const INPUT: u16 = 0x20;
/// Open flag: write.
pub const OUTPUT: u16 = 0x10;
/// Open flag: update.
pub const UPDATE: u16 = 0x08;
/// Open flag: delete.
pub const DELETE: u16 = 0x04;
/// Flag bits always on.
const BASE: u16 = 0x1002;

/// The length of the fixed part.
const FIXED: usize = 83;

/// The fixed part's fields QS/400 reads and writes: offset, size, name.
pub const LAYOUT: &[(usize, usize, &str)] = &[
    (0, 10, "file"),
    (10, 2, "library_id"),
    (12, 10, "library"),
    (22, 2, "member_id"),
    (24, 10, "member"),
    (46, 2, "open_flags"),
    (48, 4, "release"),
    (80, 2, "entry_marker"),
];

/// An open request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ufcb {
    /// The file.
    pub file: String,
    /// The library (`*LIBL` allowed).
    pub library: String,
    /// The member (`*FIRST` allowed).
    pub member: String,
    /// The open flags.
    pub flags: u16,
    /// Arrival sequence rather than keyed.
    pub arrival: bool,
    /// The `0x3A` entry: `0xC0` for read or write only, `0x40` for an
    /// open that updates or deletes.
    pub share: u8,
    /// Records per block.
    pub blocking: u16,
    /// The record format.
    pub format: String,
}

impl Ufcb {
    /// The share entry an open with `flags` uses.
    pub fn share_for(flags: u16) -> u8 {
        if flags & (UPDATE | DELETE) == 0 { 0xC0 } else { 0x40 }
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(110);
        b.extend(ebcdic_fixed(&self.file, 10));
        b.extend(0x0048u16.to_be_bytes());
        b.extend(ebcdic_fixed(&self.library, 10));
        b.extend(0x0049u16.to_be_bytes());
        b.extend(ebcdic_fixed(&self.member, 10));
        b.extend([0u8; 12]);
        b.extend((BASE | self.flags).to_be_bytes());
        b.extend(crate::wire::ebcdic("0100"));
        b.resize(FIXED, 0);
        b[56] = 0x20;
        b[60] = 0x02;
        b[81] = 0x06;
        if self.flags & INPUT != 0 {
            b.extend([0x00, 0x3C, if self.arrival { 0x80 } else { 0x00 }]);
        }
        b.extend([0x00, 0x3A, self.share]);
        b.extend(self.blocking.to_be_bytes());
        b.extend([0x00, 0x09, 0x00, 0x01, 0x00, 0x01]);
        b.extend(ebcdic_fixed(&self.format, 10));
        b.extend([0x7F, 0xFF]);
        b
    }

    /// Read one.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < FIXED + 2 {
            return Err(Error::Short {
                what: "S38UFCB",
                need: FIXED + 2,
                have: b.len(),
            });
        }
        let tail = &b[FIXED..];
        let arrival = find(tail, &[0x00, 0x3C]).is_none_or(|i| tail.get(i + 2) == Some(&0x80));
        let share = find(tail, &[0x00, 0x3A]).and_then(|i| tail.get(i + 2).copied()).unwrap_or(0xC0);
        let (blocking, format) = match find(tail, &[0x00, 0x09, 0x00, 0x01, 0x00, 0x01]) {
            Some(i) if i >= 2 => (
                u16::from_be_bytes([tail[i - 2], tail[i - 1]]),
                tail.get(i + 6..i + 16).map(text).unwrap_or_default(),
            ),
            _ => (1, String::new()),
        };
        Ok(Self {
            file: text(&b[0..10]),
            library: text(&b[12..22]),
            member: text(&b[24..34]),
            flags: u16::from_be_bytes([b[46], b[47]]) & !BASE,
            arrival,
            share,
            blocking,
            format,
        })
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_jtopen_ufcbs() {
        let read = "d1e3f4c6c6c4404040400048d8e3c5d4d7404040404000495cc6c9d9e2e3404040400000000000000000000000001022f0f1f0f000000000200000000200000000000000000000000000000000000000000600003c80003ac00064000900010001d8e6c8c4d9c6c6c440407fff";
        let b: Vec<u8> = (0..read.len() / 2)
            .map(|i| u8::from_str_radix(&read[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        let u = Ufcb::decode(&b).unwrap();
        assert_eq!(u.library, "QTEMP");
        assert_eq!(u.member, "*FIRST");
        assert_eq!(u.flags, INPUT);
        assert!(u.arrival);
        assert_eq!(u.blocking, 100);
        assert_eq!(u.format, "QWHDRFFD");
        assert_eq!(u.encode(), b);
    }

    #[test]
    fn keyed_round_trip() {
        let u = Ufcb {
            file: "QSDDMT".into(),
            library: "QSTEST".into(),
            member: "QSDDMT".into(),
            flags: INPUT | OUTPUT | UPDATE | DELETE,
            arrival: false,
            share: 0x40,
            blocking: 1,
            format: "QSDDMTR".into(),
        };
        assert_eq!(Ufcb::decode(&u.encode()).unwrap(), u);
    }
}
