// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SqlInfAttr` and the tagged entries of a `*SQLINF`'s byte space. See
//! `docs/DB_FORMAT.md` §8.1.

use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Magic at offset 0 of the attribute area.
pub const MAGIC: &[u8; 8] = b"QS400SQI";
/// Version this code writes.
pub const VERSION: u16 = 1;
/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 32;
/// Bytes of an entry header.
pub const ENTRY_HEADER_LEN: usize = 8;

/// The attribute layout.
pub static ATTR_LAYOUT: Layout = Layout {
    name: "SqlInfAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("magic", 0, 8),
        field("version", 8, 2),
        field("kind", 10, 1),
        field("reserved", 11, 1),
        field("entry_count", 12, 4),
        field("body_len", 16, 8),
        field("reserved2", 24, 8),
    ],
};

/// The entry header layout.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "SqlEntry",
    size: Some(ENTRY_HEADER_LEN),
    fields: &[field("tag", 0, 2), field("reserved", 2, 2), field("len", 4, 4)],
};

/// What SQL object the owner is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// A table (`*FILE` PF).
    Table = 1,
    /// A view (`*FILE` LF).
    View = 2,
    /// An index (`*FILE` LF).
    Index = 3,
    /// An alias (`*FILE` DDMF).
    Alias = 4,
    /// A procedure (`*PGM` or the external program's).
    Procedure = 5,
    /// A function (`*SRVPGM`).
    Function = 6,
    /// A trigger program (`*PGM`).
    Trigger = 7,
    /// A sequence (`*DTAARA`).
    Sequence = 8,
    /// A global variable (`*SRVPGM`).
    Variable = 9,
    /// A user-defined type (`*SQLUDT`).
    Type = 10,
    /// A materialized query table (`*FILE` PF).
    Mqt = 11,
    /// A schema: only in a saved library's `sqlinf` part (a `*LIB` owns no
    /// `*SQLINF`; its SQL name is in QSYS's name index).
    Schema = 12,
    /// A program or service program made with embedded SQL.
    Program = 13,
}

impl Kind {
    /// From the stored byte.
    #[must_use]
    pub fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Table,
            2 => Self::View,
            3 => Self::Index,
            4 => Self::Alias,
            5 => Self::Procedure,
            6 => Self::Function,
            7 => Self::Trigger,
            8 => Self::Sequence,
            9 => Self::Variable,
            10 => Self::Type,
            11 => Self::Mqt,
            12 => Self::Schema,
            13 => Self::Program,
            _ => return None,
        })
    }
}

/// Entry tags. Bodies are specified in `docs/DB_FORMAT.md` §8.1; a reader
/// skips tags it does not know.
pub mod tag {
    /// The SQL name: UTF-8.
    pub const NAME: u16 = 1;
    /// The definition text (the `CREATE` statement as given): UTF-8.
    pub const TEXT: u16 = 2;
    /// One column's SQL attributes.
    pub const COLUMN: u16 = 3;
    /// One constraint.
    pub const CONSTRAINT: u16 = 4;
    /// One trigger on the table.
    pub const TRIGGER: u16 = 5;
    /// Sequence or identity state.
    pub const SEQUENCE: u16 = 6;
    /// A routine's attributes.
    pub const ROUTINE: u16 = 7;
    /// One routine parameter.
    pub const PARM: u16 = 8;
    /// A global variable's attributes.
    pub const VARIABLE: u16 = 9;
    /// A user-defined type's attributes.
    pub const TYPE: u16 = 10;
    /// An index's SQL attributes (sparse condition, key expressions).
    pub const INDEX: u16 = 11;
    /// One object this one depends on.
    pub const DEPENDENCY: u16 = 12;
    /// A view's attributes (check option, query text kept apart from `TEXT`).
    pub const VIEW: u16 = 13;
    /// The `SET OPTION` values a routine or trigger was created with.
    pub const OPTIONS: u16 = 14;
    /// One embedded statement of a program.
    pub const STATEMENT: u16 = 15;
    /// One column of a distinct type: the field and its type.
    pub const COLTYPE: u16 = 16;
    /// One column's statistics of one member (a physical file's).
    pub const COLSTATS: u16 = 17;
}

/// One tagged entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What it is.
    pub tag: u16,
    /// Its body.
    pub body: Vec<u8>,
}

/// Encode entries into the byte-space image.
#[must_use]
pub fn encode(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        out.extend_from_slice(&e.tag.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&u32::try_from(e.body.len()).expect("entry fits").to_le_bytes());
        out.extend_from_slice(&e.body);
    }
    out
}

/// Decode `count` entries from the byte-space image.
pub fn decode(bytes: &[u8], count: u32) -> Result<Vec<Entry>> {
    let bad = |what: &str| DbError::Corrupt { what: format!("SQL information: {what}") };
    let mut out = Vec::with_capacity(count as usize);
    let mut at = 0usize;
    for _ in 0..count {
        let head = bytes.get(at..at + ENTRY_HEADER_LEN).ok_or_else(|| bad("entry header"))?;
        let tag = u16::from_le_bytes([head[0], head[1]]);
        let len = u32::from_le_bytes(head[4..8].try_into().expect("4")) as usize;
        at += ENTRY_HEADER_LEN;
        let body = bytes.get(at..at + len).ok_or_else(|| bad("entry body"))?;
        out.push(Entry { tag, body: body.to_vec() });
        at += len;
    }
    Ok(out)
}

/// Encode the attribute area.
#[must_use]
pub fn encode_attr(kind: Kind, count: u32, body_len: u64) -> [u8; ATTR_LEN] {
    let mut b = [0u8; ATTR_LEN];
    b[..8].copy_from_slice(MAGIC);
    b[8..10].copy_from_slice(&VERSION.to_le_bytes());
    b[10] = kind as u8;
    b[12..16].copy_from_slice(&count.to_le_bytes());
    b[16..24].copy_from_slice(&body_len.to_le_bytes());
    b
}

/// Decode the attribute area: kind, entry count, body bytes.
pub fn decode_attr(b: &[u8]) -> Result<(Kind, u32, u64)> {
    let bad = |what: &str| DbError::Corrupt { what: format!("SQL information: {what}") };
    if b.len() < ATTR_LEN || &b[..8] != MAGIC {
        return Err(bad("magic"));
    }
    if u16::from_le_bytes([b[8], b[9]]) != VERSION {
        return Err(bad("version"));
    }
    let kind = Kind::from_u8(b[10]).ok_or_else(|| bad("kind"))?;
    let count = u32::from_le_bytes(b[12..16].try_into().expect("4"));
    let len = u64::from_le_bytes(b[16..24].try_into().expect("8"));
    Ok((kind, count, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let es = vec![
            Entry { tag: tag::NAME, body: b"CUSTOMER_MASTER".to_vec() },
            Entry { tag: tag::TEXT, body: Vec::new() },
            Entry { tag: 999, body: vec![1, 2, 3] },
        ];
        let b = encode(&es);
        assert_eq!(decode(&b, 3).unwrap(), es);
        assert!(decode(&b[..b.len() - 1], 3).is_err());
        let a = encode_attr(Kind::View, 3, b.len() as u64);
        assert_eq!(decode_attr(&a).unwrap(), (Kind::View, 3, b.len() as u64));
    }
}
