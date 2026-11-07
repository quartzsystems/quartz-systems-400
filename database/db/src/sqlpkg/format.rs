// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*SQLPKG` payload (`docs/DB_FORMAT.md` §9): the attribute
//! (`SqlPkgAttr`) and the statements (`SqlPkgStmt`).

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// The payload version.
pub const VERSION: u16 = 1;

/// Bytes of the attribute.
pub const ATTR_LEN: usize = 64;

/// Bytes of a statement's fixed part.
pub const STMT_FIXED: usize = 48;

/// The attribute layout.
pub static ATTR_LAYOUT: Layout = Layout {
    name: "SqlPkgAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("version", 0, 2),
        field("kind", 2, 1),
        field("naming", 3, 1),
        field("commit", 4, 1),
        field("date_format", 5, 1),
        field("date_sep", 6, 1),
        field("time_format", 7, 1),
        field("time_sep", 8, 1),
        field("decimal_point", 9, 1),
        field("reserved", 10, 2),
        field("stmt_count", 12, 4),
        field("space_len", 16, 8),
        field("token", 24, 8),
        field("default_collection", 32, 10),
        field("reserved2", 42, 22),
    ],
};

/// The statement layout.
pub static STMT_LAYOUT: Layout = Layout {
    name: "SqlPkgStmt",
    size: None,
    fields: &[
        field("len", 0, 4),
        field("name", 4, 18),
        field("stmt_type", 22, 1),
        field("flags", 23, 1),
        field("section", 24, 2),
        field("reserved", 26, 2),
        field("text_len", 28, 4),
        field("cols_len", 32, 4),
        field("markers_len", 36, 4),
        field("reserved2", 40, 8),
        Field {
            name: "parts",
            offset: Some(48),
            size: None,
        },
    ],
};

/// Who made a package.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PkgKind {
    /// The database host server's extended dynamic support.
    #[default]
    Extended,
    /// A DRDA requester's bind.
    Bound,
}

/// A package's attributes: the environment its statements were prepared in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PkgAttr {
    /// Who made it.
    pub kind: PkgKind,
    /// `*SQL` naming.
    pub sql_naming: bool,
    /// Commitment control: 0 `*NONE`, 1 `*CHG`, 2 `*CS`, 3 `*ALL`, 4 `*RR`.
    pub commit: u8,
    /// The date format code.
    pub date_format: u8,
    /// The date separator.
    pub date_sep: u8,
    /// The time format code.
    pub time_format: u8,
    /// The time separator.
    pub time_sep: u8,
    /// The decimal point.
    pub decimal_point: u8,
    /// The consistency token.
    pub token: u64,
    /// The default collection, empty for none.
    pub default_collection: String,
}

/// A statement of a package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PkgStmt {
    /// Its name (at most 18 characters).
    pub name: String,
    /// The host server's statement type; 0 for a DRDA section.
    pub stmt_type: u8,
    /// It needs the default collection.
    pub needs_collection: bool,
    /// The DRDA section number, 0 for none.
    pub section: u16,
    /// The text.
    pub text: String,
    /// Its result columns as an SQLDA, empty for none.
    pub cols: Vec<u8>,
    /// Its parameter markers as an SQLDA, empty for none.
    pub markers: Vec<u8>,
}

fn bad(what: &str) -> DbError {
    DbError::Corrupt {
        what: format!("SQL package: {what}"),
    }
}

fn padded(s: &str, n: usize) -> Vec<u8> {
    let mut b: Vec<u8> = s.bytes().take(n).collect();
    b.resize(n, b' ');
    b
}

fn trimmed(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim_end().to_string()
}

fn u32_of(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("four bytes")) as usize
}

fn len32(n: usize) -> [u8; 4] {
    u32::try_from(n).expect("part fits").to_le_bytes()
}

impl PkgAttr {
    /// The attribute area for `count` statements in `space_len` bytes.
    #[must_use]
    pub fn encode(&self, count: usize, space_len: u64) -> Vec<u8> {
        let mut b = vec![0u8; ATTR_LEN];
        b[0..2].copy_from_slice(&VERSION.to_le_bytes());
        b[2] = u8::from(self.kind == PkgKind::Bound);
        b[3] = u8::from(self.sql_naming);
        b[4] = self.commit;
        b[5] = self.date_format;
        b[6] = self.date_sep;
        b[7] = self.time_format;
        b[8] = self.time_sep;
        b[9] = self.decimal_point;
        b[12..16].copy_from_slice(&len32(count));
        b[16..24].copy_from_slice(&space_len.to_le_bytes());
        b[24..32].copy_from_slice(&self.token.to_le_bytes());
        b[32..42].copy_from_slice(&padded(&self.default_collection, 10));
        b
    }

    /// From the attribute area: the attributes, the statement count and
    /// the bytes of statements.
    pub fn decode(b: &[u8]) -> Result<(Self, usize, u64)> {
        if b.len() < ATTR_LEN {
            return Err(bad("short attribute"));
        }
        let version = u16::from_le_bytes([b[0], b[1]]);
        if version != VERSION {
            return Err(bad(&format!("version {version}")));
        }
        let a = Self {
            kind: if b[2] == 1 {
                PkgKind::Bound
            } else {
                PkgKind::Extended
            },
            sql_naming: b[3] == 1,
            commit: b[4],
            date_format: b[5],
            date_sep: b[6],
            time_format: b[7],
            time_sep: b[8],
            decimal_point: b[9],
            token: u64::from_le_bytes(b[24..32].try_into().expect("eight bytes")),
            default_collection: trimmed(&b[32..42]),
        };
        let space = u64::from_le_bytes(b[16..24].try_into().expect("eight bytes"));
        Ok((a, u32_of(b, 12), space))
    }
}

impl PkgStmt {
    /// The entry's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; STMT_FIXED];
        b[4..22].copy_from_slice(&padded(&self.name, 18));
        b[22] = self.stmt_type;
        b[23] = u8::from(self.needs_collection);
        b[24..26].copy_from_slice(&self.section.to_le_bytes());
        b[28..32].copy_from_slice(&len32(self.text.len()));
        b[32..36].copy_from_slice(&len32(self.cols.len()));
        b[36..40].copy_from_slice(&len32(self.markers.len()));
        b.extend_from_slice(self.text.as_bytes());
        b.extend_from_slice(&self.cols);
        b.extend_from_slice(&self.markers);
        let n = b.len();
        b[0..4].copy_from_slice(&len32(n));
        b
    }

    /// `count` entries from the byte space.
    #[allow(clippy::many_single_char_names)] // the parts' lengths
    pub fn decode_all(b: &[u8], count: usize) -> Result<Vec<Self>> {
        let mut out = Vec::with_capacity(count);
        let mut at = 0;
        for _ in 0..count {
            if b.len() < at + STMT_FIXED {
                return Err(bad("short statement"));
            }
            let e = &b[at..];
            let len = u32_of(e, 0);
            let (t, c, m) = (u32_of(e, 28), u32_of(e, 32), u32_of(e, 36));
            if len != STMT_FIXED + t + c + m || e.len() < len {
                return Err(bad("statement length"));
            }
            let parts = &e[STMT_FIXED..len];
            out.push(Self {
                name: trimmed(&e[4..22]),
                stmt_type: e[22],
                needs_collection: e[23] & 1 == 1,
                section: u16::from_le_bytes([e[24], e[25]]),
                text: String::from_utf8(parts[..t].to_vec()).map_err(|_| bad("text"))?,
                cols: parts[t..t + c].to_vec(),
                markers: parts[t + c..].to_vec(),
            });
            at += len;
        }
        Ok(out)
    }
}
