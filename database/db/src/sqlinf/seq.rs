// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bodies of the `COLUMN` and `SEQUENCE` entries (`docs/DB_FORMAT.md`
//! §8.1, `SqlColumn` and `SqlSequence`): an identity column's or
//! sequence's full attributes, which `FieldEntry`'s four-byte start and
//! increment cannot hold, a generated column's kind and expression, and
//! a sequence's next value.

use qsname::Name;

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// Bytes of the fixed part of a `COLUMN` body.
pub const COLUMN_LEN: usize = 128;
/// Bytes of the fixed part of a `SEQUENCE` body.
pub const SEQUENCE_LEN: usize = 128;

/// The `COLUMN` body layout.
pub static COLUMN_LAYOUT: Layout = Layout {
    name: "SqlColumn",
    size: None,
    fields: &[
        field("field", 0, 10),
        field("kind", 10, 1),
        field("flags", 11, 1),
        field("cache", 12, 4),
        field("start", 16, 16),
        field("increment", 32, 16),
        field("min", 48, 16),
        field("max", 64, 16),
        field("text_len", 80, 2),
        field("reserved", 82, 46),
        Field { name: "text", offset: Some(128), size: None },
    ],
};

/// The `SEQUENCE` body layout.
pub static SEQUENCE_LAYOUT: Layout = Layout {
    name: "SqlSequence",
    size: Some(SEQUENCE_LEN),
    fields: &[
        field("data_type", 0, 1),
        field("flags", 1, 1),
        field("precision", 2, 1),
        field("reserved", 3, 1),
        field("cache", 4, 4),
        field("start", 8, 16),
        field("increment", 24, 16),
        field("min", 40, 16),
        field("max", 56, 16),
        field("next", 72, 16),
        field("reserved2", 88, 40),
    ],
};

/// An identity column's or sequence's attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SeqAttrs {
    /// `START WITH`.
    pub start: i128,
    /// `INCREMENT BY`, never 0.
    pub increment: i128,
    /// `MINVALUE`.
    pub min: i128,
    /// `MAXVALUE`.
    pub max: i128,
    /// `CYCLE`.
    pub cycle: bool,
    /// `CACHE n`, 0 for `NO CACHE`.
    pub cache: u32,
    /// `ORDER`.
    pub order: bool,
    /// `GENERATED ALWAYS` (identity columns).
    pub always: bool,
}

impl SeqAttrs {
    /// The value after `v`, or `None` when the range is used up and the
    /// attributes do not cycle (`SQL0359`).
    #[must_use]
    pub fn after(&self, v: i128) -> Option<i128> {
        let n = v.checked_add(self.increment);
        match n {
            Some(n) if n >= self.min && n <= self.max => Some(n),
            _ if self.cycle => Some(if self.increment > 0 { self.min } else { self.max }),
            _ => None,
        }
    }

    /// Whether `v` lies in the range.
    #[must_use]
    pub fn holds(&self, v: i128) -> bool {
        v >= self.min && v <= self.max
    }

    fn put(&self, b: &mut [u8]) {
        b[0..16].copy_from_slice(&self.start.to_le_bytes());
        b[16..32].copy_from_slice(&self.increment.to_le_bytes());
        b[32..48].copy_from_slice(&self.min.to_le_bytes());
        b[48..64].copy_from_slice(&self.max.to_le_bytes());
    }

    fn get(b: &[u8], flags: u8, cache: u32) -> Self {
        let i = |at: usize| i128::from_le_bytes(b[at..at + 16].try_into().expect("16"));
        Self {
            start: i(0),
            increment: i(16),
            min: i(32),
            max: i(48),
            always: flags & 1 != 0,
            cycle: flags & 2 != 0,
            order: flags & 4 != 0,
            cache,
        }
    }

    fn flags(&self) -> u8 {
        u8::from(self.always) | u8::from(self.cycle) << 1 | u8::from(self.order) << 2
    }
}

/// What a column's values come from, besides what the record says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    /// Nothing generated: the entry keeps `IMPLICITLY HIDDEN` alone.
    Plain,
    /// An identity column.
    Identity(SeqAttrs),
    /// `ROW CHANGE TIMESTAMP`: `always` or `BY DEFAULT`.
    RowChange(bool),
    /// `GENERATED ALWAYS AS (expression)`: the expression's text.
    Expr(String),
    /// An auditing column (`DATA CHANGE OPERATION`, `SESSION_USER`, ...):
    /// `always`, what it records.
    Audit(bool, String),
}

/// One column's SQL attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlColumn {
    /// The field.
    pub field: Name,
    /// Its kind.
    pub kind: ColumnKind,
    /// `IMPLICITLY HIDDEN`.
    pub hidden: bool,
}

impl SqlColumn {
    /// The `COLUMN` entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; COLUMN_LEN];
        b[..10].copy_from_slice(self.field.as_bytes());
        let (kind, text) = match &self.kind {
            ColumnKind::Plain => (0, ""),
            ColumnKind::Identity(a) => {
                b[11] = a.flags();
                b[12..16].copy_from_slice(&a.cache.to_le_bytes());
                a.put(&mut b[16..80]);
                (1, "")
            }
            ColumnKind::RowChange(always) => {
                b[11] = u8::from(*always);
                (2, "")
            }
            ColumnKind::Expr(t) => (3, t.as_str()),
            ColumnKind::Audit(always, t) => {
                b[11] = u8::from(*always);
                (4, t.as_str())
            }
        };
        b[10] = kind;
        if self.hidden {
            b[11] |= 8;
        }
        b[80..82].copy_from_slice(&u16::try_from(text.len()).expect("short").to_le_bytes());
        b.extend_from_slice(text.as_bytes());
        b
    }

    /// From a `COLUMN` entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = || DbError::Corrupt { what: "column entry".into() };
        if b.len() < COLUMN_LEN {
            return Err(bad());
        }
        let n = usize::from(u16::from_le_bytes([b[80], b[81]]));
        let text = String::from_utf8(b.get(COLUMN_LEN..COLUMN_LEN + n).ok_or_else(bad)?.to_vec()).map_err(|_| bad())?;
        let flags = b[11];
        let kind = match b[10] {
            0 => ColumnKind::Plain,
            1 => ColumnKind::Identity(SeqAttrs::get(&b[16..80], flags, u32::from_le_bytes(b[12..16].try_into().expect("4")))),
            2 => ColumnKind::RowChange(flags & 1 != 0),
            3 => ColumnKind::Expr(text),
            4 => ColumnKind::Audit(flags & 1 != 0, text),
            _ => return Err(bad()),
        };
        Ok(Self { field: Name::from_bytes(b[..10].try_into().expect("10")), kind, hidden: flags & 8 != 0 })
    }
}

/// A sequence: its type, attributes and the next value it gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sequence {
    /// 1 `SMALLINT`, 2 `INTEGER`, 3 `BIGINT`, 4 `DECIMAL`, 5 `NUMERIC`.
    pub data_type: u8,
    /// Digits of a `DECIMAL` or `NUMERIC` sequence.
    pub precision: u8,
    /// The attributes.
    pub attrs: SeqAttrs,
    /// The value the next `NEXT VALUE` gives.
    pub next: i128,
    /// The range is used up: the next `NEXT VALUE` fails.
    pub exhausted: bool,
}

impl Sequence {
    /// The `SEQUENCE` entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; SEQUENCE_LEN];
        b[0] = self.data_type;
        b[1] = self.attrs.flags() | u8::from(self.exhausted) << 3;
        b[2] = self.precision;
        b[4..8].copy_from_slice(&self.attrs.cache.to_le_bytes());
        self.attrs.put(&mut b[8..72]);
        b[72..88].copy_from_slice(&self.next.to_le_bytes());
        b
    }

    /// From a `SEQUENCE` entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < SEQUENCE_LEN {
            return Err(DbError::Corrupt { what: "sequence entry".into() });
        }
        Ok(Self {
            data_type: b[0],
            precision: b[2],
            attrs: SeqAttrs::get(&b[8..72], b[1], u32::from_le_bytes(b[4..8].try_into().expect("4"))),
            next: i128::from_le_bytes(b[72..88].try_into().expect("16")),
            exhausted: b[1] & 8 != 0,
        })
    }
}


/// The SQL attributes of `file`'s columns.
pub fn columns(txn: &mut store::txn::Txn<'_>, file: store::obj::ObjectId) -> Result<Vec<SqlColumn>> {
    let Some(info) = super::load(txn, file)? else { return Ok(Vec::new()) };
    info.all(super::tag::COLUMN).map(SqlColumn::decode).collect()
}

/// Replace the SQL attributes of `file`'s columns.
pub fn store_columns(txn: &mut store::txn::Txn<'_>, file: store::obj::ObjectId, cols: &[SqlColumn], now: u64) -> Result<()> {
    let mut info = super::load(txn, file)?.unwrap_or_else(|| super::SqlInfo::new(super::Kind::Table));
    info.remove(super::tag::COLUMN);
    for c in cols {
        info.push(super::tag::COLUMN, c.encode());
    }
    super::save(txn, file, &info, now)
}

/// The largest and smallest values a field holds, for an identity
/// column's default range.
#[must_use]
pub fn field_range(f: &dds::FieldDef) -> (i128, i128) {
    if f.int_range {
        return match f.byte_len {
            2 => (i128::from(i16::MIN), i128::from(i16::MAX)),
            4 => (i128::from(i32::MIN), i128::from(i32::MAX)),
            _ => (i128::from(i64::MIN), i128::from(i64::MAX)),
        };
    }
    let digits = f.length.saturating_sub(u32::from(f.decimals)).min(38);
    let top = 10i128.pow(digits) - 1;
    (-top, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_round_trip() {
        let a = SeqAttrs { start: -5, increment: -2, min: i128::from(i64::MIN), max: -1, cycle: true, cache: 20, order: false, always: true };
        let c = SqlColumn { field: Name::parse("ID").unwrap(), kind: ColumnKind::Identity(a), hidden: true };
        assert_eq!(SqlColumn::decode(&c.encode()).unwrap(), c);
        let e = SqlColumn { field: Name::parse("TS").unwrap(), kind: ColumnKind::Audit(true, "SESSION_USER".into()), hidden: false };
        assert_eq!(SqlColumn::decode(&e.encode()).unwrap(), e);
        let s = Sequence { data_type: 3, precision: 0, attrs: a, next: -7, exhausted: true };
        assert_eq!(Sequence::decode(&s.encode()).unwrap(), s);
        assert_eq!(a.after(-1), Some(-3));
        assert_eq!(a.after(i128::from(i64::MIN)), Some(-1));
    }
}
