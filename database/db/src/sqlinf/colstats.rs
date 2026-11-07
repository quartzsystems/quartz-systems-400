// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Column statistics kept on a physical file's `*SQLINF`: one `COLSTATS`
//! entry per member and column, so they outlive an IPL. Values are kept
//! as a kind and bytes the SQL layer gives and reads back. See
//! `docs/DB_FORMAT.md` §8.1, `SqlColStats` and `SqlStatValue`.

use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// Bytes of the fixed part of a `COLSTATS` body.
pub const COLSTATS_FIXED: usize = 96;
/// Bytes of the fixed part of a value.
pub const VALUE_FIXED: usize = 8;

/// The `COLSTATS` body layout.
pub static COLSTATS_LAYOUT: Layout = Layout {
    name: "SqlColStats",
    size: None,
    fields: &[
        field("member", 0, 16),
        field("field", 16, 10),
        field("column_len", 26, 2),
        field("bound_count", 28, 2),
        field("frequent_count", 30, 2),
        field("distinct", 32, 8),
        field("nulls", 40, 8),
        field("avg_len", 48, 4),
        field("max_len", 52, 4),
        field("rows", 56, 8),
        field("deleted", 64, 8),
        field("slots", 72, 8),
        field("created", 80, 8),
        field("updated", 88, 8),
        Field {
            name: "column",
            offset: Some(96),
            size: None,
        },
    ],
};

/// The layout of one value of a `COLSTATS` body.
pub static VALUE_LAYOUT: Layout = Layout {
    name: "SqlStatValue",
    size: None,
    fields: &[
        field("kind", 0, 1),
        field("reserved", 1, 3),
        field("len", 4, 4),
        Field {
            name: "bytes",
            offset: Some(8),
            size: None,
        },
    ],
};

/// A value as kept: its kind (§8.1) and bytes.
pub type StatValue = (u8, Vec<u8>);

/// One column's statistics of one member.
#[derive(Clone, Debug, PartialEq)]
pub struct ColStats {
    /// The member.
    pub member: ObjectId,
    /// The column's system name.
    pub field: qsname::Name,
    /// Its SQL name.
    pub column: String,
    /// Distinct non-null values.
    pub distinct: u64,
    /// Null values.
    pub nulls: u64,
    /// Average length of the values' text.
    pub avg_len: u32,
    /// Longest.
    pub max_len: u32,
    /// Rows, deleted rows and slots of the member when collected.
    pub rows: u64,
    /// See `rows`.
    pub deleted: u64,
    /// See `rows`.
    pub slots: u64,
    /// When first collected and last updated (Unix seconds).
    pub created: u64,
    /// See `created`.
    pub updated: u64,
    /// The histogram's range bounds, ascending.
    pub bounds: Vec<StatValue>,
    /// The most frequent values and each one's share of the rows.
    pub frequent: Vec<(f64, StatValue)>,
}

fn bad() -> DbError {
    DbError::Corrupt {
        what: "SQL information: column statistics entry".into(),
    }
}

fn put_value(b: &mut Vec<u8>, (kind, bytes): &StatValue) {
    b.push(*kind);
    b.extend_from_slice(&[0, 0, 0]);
    b.extend_from_slice(
        &u32::try_from(bytes.len())
            .expect("a value fits")
            .to_le_bytes(),
    );
    b.extend_from_slice(bytes);
}

fn get_value(b: &[u8], at: &mut usize) -> Result<StatValue> {
    let head = b.get(*at..*at + VALUE_FIXED).ok_or_else(bad)?;
    let len = u32::from_le_bytes(head[4..8].try_into().expect("4")) as usize;
    let kind = head[0];
    *at += VALUE_FIXED;
    let bytes = b.get(*at..*at + len).ok_or_else(bad)?.to_vec();
    *at += len;
    Ok((kind, bytes))
}

fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8"))
}

impl ColStats {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let n16 = |n: usize| u16::try_from(n).expect("few").to_le_bytes();
        let mut b = vec![0u8; COLSTATS_FIXED];
        b[0..16].copy_from_slice(&self.member.to_bytes());
        b[16..26].copy_from_slice(self.field.as_bytes());
        b[26..28].copy_from_slice(&n16(self.column.len()));
        b[28..30].copy_from_slice(&n16(self.bounds.len()));
        b[30..32].copy_from_slice(&n16(self.frequent.len()));
        for (at, v) in [
            (32, self.distinct),
            (40, self.nulls),
            (56, self.rows),
            (64, self.deleted),
            (72, self.slots),
            (80, self.created),
            (88, self.updated),
        ] {
            b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        }
        b[48..52].copy_from_slice(&self.avg_len.to_le_bytes());
        b[52..56].copy_from_slice(&self.max_len.to_le_bytes());
        b.extend_from_slice(self.column.as_bytes());
        for v in &self.bounds {
            put_value(&mut b, v);
        }
        for (share, v) in &self.frequent {
            b.extend_from_slice(&share.to_le_bytes());
            put_value(&mut b, v);
        }
        b
    }

    /// From an entry body.
    ///
    /// # Errors
    ///
    /// A body too short for what it says it holds.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < COLSTATS_FIXED {
            return Err(bad());
        }
        let n16 = |at: usize| usize::from(u16::from_le_bytes([b[at], b[at + 1]]));
        let mut at = COLSTATS_FIXED + n16(26);
        let column = b
            .get(COLSTATS_FIXED..at)
            .and_then(|s| String::from_utf8(s.to_vec()).ok())
            .ok_or_else(bad)?;
        let mut bounds = Vec::with_capacity(n16(28));
        for _ in 0..n16(28) {
            bounds.push(get_value(b, &mut at)?);
        }
        let mut frequent = Vec::with_capacity(n16(30));
        for _ in 0..n16(30) {
            let share =
                f64::from_le_bytes(b.get(at..at + 8).ok_or_else(bad)?.try_into().expect("8"));
            at += 8;
            frequent.push((share, get_value(b, &mut at)?));
        }
        Ok(Self {
            member: ObjectId::from_bytes(&b[0..16]),
            field: qsname::Name::from_bytes(b[16..26].try_into().expect("10 bytes")),
            column,
            distinct: u64_at(b, 32),
            nulls: u64_at(b, 40),
            avg_len: u32::from_le_bytes(b[48..52].try_into().expect("4")),
            max_len: u32::from_le_bytes(b[52..56].try_into().expect("4")),
            rows: u64_at(b, 56),
            deleted: u64_at(b, 64),
            slots: u64_at(b, 72),
            created: u64_at(b, 80),
            updated: u64_at(b, 88),
            bounds,
            frequent,
        })
    }
}

/// The statistics kept on `file`.
///
/// # Errors
///
/// The file's SQL information cannot be read.
pub fn load(txn: &mut Txn<'_>, file: ObjectId) -> Result<Vec<ColStats>> {
    let Some(info) = super::load(txn, file)? else {
        return Ok(Vec::new());
    };
    info.all(super::tag::COLSTATS)
        .map(ColStats::decode)
        .collect()
}

/// Keep `stats` on `file`, each in place of the entry of its member and
/// column; entries of members not in `members` (gone, or of the file
/// before a restore) go. A file without SQL information gets it.
///
/// # Errors
///
/// The file's SQL information cannot be read or written.
pub fn store(
    txn: &mut Txn<'_>,
    file: ObjectId,
    members: &[ObjectId],
    stats: &[ColStats],
    now: u64,
) -> Result<()> {
    let mut info =
        super::load(txn, file)?.unwrap_or_else(|| super::SqlInfo::new(super::Kind::Table));
    info.entries.retain(|e| {
        e.tag != super::tag::COLSTATS
            || ColStats::decode(&e.body).is_ok_and(|old| {
                members.contains(&old.member)
                    && !stats
                        .iter()
                        .any(|s| s.member == old.member && s.field == old.field)
            })
    });
    for s in stats {
        info.push(super::tag::COLSTATS, s.encode());
    }
    super::save(txn, file, &info, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_comes_back_as_written() {
        let s = ColStats {
            member: ObjectId::from_bytes(&[7; 16]),
            field: qsname::Name::parse("CUSNO").unwrap(),
            column: "CUSTOMER_NUMBER".into(),
            distinct: 99,
            nulls: 3,
            avg_len: 5,
            max_len: 9,
            rows: 1000,
            deleted: 2,
            slots: 1002,
            created: 10,
            updated: 20,
            bounds: vec![(1, b"10".to_vec()), (0, Vec::new())],
            frequent: vec![(0.25, (2, "ab".into()))],
        };
        let b = s.encode();
        assert_eq!(
            b.len(),
            COLSTATS_FIXED + 15 + (VALUE_FIXED + 2) + VALUE_FIXED + 8 + VALUE_FIXED + 2
        );
        assert_eq!(ColStats::decode(&b).unwrap(), s);
        assert!(ColStats::decode(&b[..b.len() - 1]).is_err());
    }
}
