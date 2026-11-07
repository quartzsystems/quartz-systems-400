// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The saved names on disk: the data of LIC partition TLV `0x010A`,
//! little-endian (docs/PHASE1.md, "Hardware resource names").
//!
//! ```text
//! 0   magic "QSHN"     4
//! 4   version  u16     2   1
//! 6   count    u16     2
//! 8   entries, each:
//!     0   kind     u8      the protocol's numbers, 0 bridge .. 13 channel
//!     1   reserved u8      0
//!     2   name     10      ASCII, blank-padded
//!     12  parent   10      packaging parent, blank when none
//!     22  logical  10      logical parent, blank when none
//!     32  type     4       blank when none
//!     36  model    3       blank when none
//!     39  key_len  u8
//!     40  key              ASCII, at most 200 bytes
//!     ..  loc_len  u8
//!     ..  location         ASCII
//! ```

use thiserror::Error;

use super::{Entry, NameMap};
use crate::table::Kind;

/// The TLV tag of the saved names in the LIC record.
pub const TAG_HW_NAMES: u16 = 0x010A;
/// Format version.
pub const VERSION: u16 = 1;
/// Most bytes the names may take of the LIC record's 64 KiB.
pub const MAX_BYTES: usize = 24 * 1024;

const MAGIC: [u8; 4] = *b"QSHN";
const HEADER: usize = 8;

/// A saved map that does not read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FormatError {
    /// Not the saved names.
    #[error("bad magic")]
    Magic,
    /// A version this release does not read.
    #[error("unsupported version {0}")]
    Version(u16),
    /// Shorter than its counts say.
    #[error("truncated")]
    Truncated,
    /// A kind number no release has.
    #[error("unknown kind {0}")]
    Kind(u8),
    /// Bytes after the last entry.
    #[error("trailing bytes")]
    Trailing,
}

fn kind_code(k: Kind) -> u8 {
    match k {
        Kind::Bridge => 0,
        Kind::Lan => 1,
        Kind::Storage => 2,
        Kind::Disk => 3,
        Kind::Optical => 4,
        Kind::Comm => 5,
        Kind::Port => 6,
        Kind::Keyboard => 7,
        Kind::Other => 8,
        Kind::Tape => 9,
        Kind::TapeLibrary => 10,
        Kind::Processor => 11,
        Kind::Memory => 12,
        Kind::Channel => 13,
    }
}

fn kind_of(code: u8) -> Result<Kind, FormatError> {
    Kind::ALL
        .into_iter()
        .find(|k| kind_code(*k) == code)
        .ok_or(FormatError::Kind(code))
}

fn fixed(out: &mut Vec<u8>, s: &str, len: usize) {
    let mut b: Vec<u8> = s.bytes().filter(u8::is_ascii).take(len).collect();
    b.resize(len, b' ');
    out.extend_from_slice(&b);
}

fn short(out: &mut Vec<u8>, s: &str) {
    let b: Vec<u8> = s.bytes().filter(u8::is_ascii).take(255).collect();
    out.push(u8::try_from(b.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(&b);
}

/// The bytes of one entry.
fn entry(e: &Entry) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + e.key.len() + e.location.len());
    out.push(kind_code(e.kind));
    out.push(0);
    fixed(&mut out, &e.name, 10);
    fixed(&mut out, &e.parent, 10);
    fixed(&mut out, &e.logical, 10);
    fixed(&mut out, &e.type_model.0, 4);
    fixed(&mut out, &e.type_model.1, 3);
    short(&mut out, &e.key);
    short(&mut out, &e.location);
    out
}

/// Encode, at most [`MAX_BYTES`]: entries past it are left off (the map
/// lists the present resources first).
#[must_use]
pub fn encode(map: &NameMap) -> Vec<u8> {
    let mut body = Vec::new();
    let mut count: u16 = 0;
    for e in &map.entries {
        let b = entry(e);
        if HEADER + body.len() + b.len() > MAX_BYTES || count == u16::MAX {
            break;
        }
        body.extend_from_slice(&b);
        count += 1;
    }
    let mut out = Vec::with_capacity(HEADER + body.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if self.0.len() < n {
            return Err(FormatError::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }

    fn text(&mut self, n: usize) -> Result<String, FormatError> {
        Ok(String::from_utf8_lossy(self.take(n)?).trim_end().to_string())
    }

    fn short(&mut self) -> Result<String, FormatError> {
        let n = usize::from(self.take(1)?[0]);
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// Decode a saved map.
pub fn decode(bytes: &[u8]) -> Result<NameMap, FormatError> {
    let mut r = Reader(bytes);
    if r.take(4)? != MAGIC {
        return Err(FormatError::Magic);
    }
    let v = r.take(2)?;
    let version = u16::from_le_bytes([v[0], v[1]]);
    if version != VERSION {
        return Err(FormatError::Version(version));
    }
    let c = r.take(2)?;
    let count = u16::from_le_bytes([c[0], c[1]]);
    let mut entries = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let head = r.take(2)?;
        let kind = kind_of(head[0])?;
        let name = r.text(10)?;
        let parent = r.text(10)?;
        let logical = r.text(10)?;
        let ty = r.text(4)?;
        let model = r.text(3)?;
        let key = r.short()?;
        let location = r.short()?;
        entries.push(Entry {
            name,
            kind,
            key,
            location,
            type_model: (ty, model),
            parent,
            logical,
        });
    }
    if !r.0.is_empty() {
        return Err(FormatError::Trailing);
    }
    Ok(NameMap { entries })
}
