// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Column statistics as a file's `*SQLINF` keeps them (`COLSTATS`
//! entries, `docs/DB_FORMAT.md` §8.1 `SqlColStats`): [`ColumnStats`] to
//! the stored form and back, values by their kind.

use db::record::datetime::{Date, Time};
use db::sqlinf::colstats::{ColStats, StatValue};
use qdecimal::{DecFloat, Decimal, DfFormat};
use store::obj::ObjectId;

use super::stats::ColumnStats;
use crate::value::{Timestamp, Value};

fn date_bytes(d: Date) -> Vec<u8> {
    let mut b = d.year.to_le_bytes().to_vec();
    b.extend([d.month, d.day]);
    b
}

fn date_of(b: &[u8]) -> Option<Date> {
    Date::new(
        u16::from_le_bytes([*b.first()?, *b.get(1)?]),
        *b.get(2)?,
        *b.get(3)?,
    )
}

fn time_of(b: &[u8]) -> Option<Time> {
    Time::new(*b.first()?, *b.get(1)?, *b.get(2)?)
}

/// A value in its stored form; `None` for one statistics do not keep
/// (an array).
#[must_use]
pub fn encode_value(v: &Value) -> Option<StatValue> {
    Some(match v {
        Value::Null => (0, Vec::new()),
        Value::Num(d) => (1, d.to_string().into_bytes()),
        Value::Str(s) => (2, s.clone().into_bytes()),
        Value::Double(x) => (3, x.to_le_bytes().to_vec()),
        Value::Real(x) => (4, x.to_le_bytes().to_vec()),
        Value::DecFloat(d) => (5, d.to_string().into_bytes()),
        Value::Date(d) => (6, date_bytes(*d)),
        Value::Time(t) => (7, vec![t.hour, t.minute, t.second]),
        Value::Timestamp(ts) => {
            let mut b = date_bytes(ts.date);
            b.extend([ts.time.hour, ts.time.minute, ts.time.second, ts.precision]);
            b.extend(ts.picos.to_le_bytes());
            (8, b)
        }
        Value::Bin(b) => (9, b.clone()),
        Value::Bool(x) => (10, vec![u8::from(*x)]),
        Value::Array(_) => return None,
    })
}

/// A stored value back; `None` for one this code cannot read.
#[must_use]
pub fn decode_value((kind, b): &StatValue) -> Option<Value> {
    let text = || std::str::from_utf8(b).ok();
    Some(match kind {
        0 => Value::Null,
        1 => Value::Num(Decimal::parse(text()?).ok()?),
        2 => Value::Str(text()?.to_string()),
        3 => Value::Double(f64::from_le_bytes(b.as_slice().try_into().ok()?)),
        4 => Value::Real(f32::from_le_bytes(b.as_slice().try_into().ok()?)),
        5 => Value::DecFloat(DecFloat::parse(text()?, DfFormat::D34).ok()?),
        6 => Value::Date(date_of(b)?),
        7 => Value::Time(time_of(b)?),
        8 => {
            let picos = u64::from_le_bytes(b.get(8..16)?.try_into().ok()?);
            Value::Timestamp(Timestamp {
                date: date_of(b)?,
                time: time_of(b.get(4..7)?)?,
                picos,
                precision: *b.get(7)?,
            })
        }
        9 => Value::Bin(b.clone()),
        10 => Value::Bool(*b.first()? != 0),
        _ => return None,
    })
}

/// The stored form of `s`, the statistics of a column of `member`.
#[must_use]
pub fn to_stored(member: ObjectId, s: &ColumnStats) -> Option<ColStats> {
    Some(ColStats {
        member,
        field: qsname::Name::parse(&s.system_column).ok()?,
        column: s.column.clone(),
        distinct: s.distinct,
        nulls: s.nulls,
        avg_len: s.avg_len,
        max_len: s.max_len,
        rows: s.rows,
        deleted: s.deleted,
        slots: s.slots,
        created: s.created,
        updated: s.updated,
        bounds: s.bounds.iter().filter_map(encode_value).collect(),
        frequent: s
            .frequent
            .iter()
            .filter_map(|(v, f)| encode_value(v).map(|e| (*f, e)))
            .collect(),
    })
}

/// Statistics back from their stored form; `None` when a value cannot
/// be read (they are collected again).
#[must_use]
pub fn from_stored(c: &ColStats) -> Option<ColumnStats> {
    Some(ColumnStats {
        column: c.column.clone(),
        system_column: c.field.to_string(),
        distinct: c.distinct,
        nulls: c.nulls,
        avg_len: c.avg_len,
        max_len: c.max_len,
        bounds: c
            .bounds
            .iter()
            .map(decode_value)
            .collect::<Option<Vec<_>>>()?,
        frequent: c
            .frequent
            .iter()
            .map(|(f, v)| decode_value(v).map(|v| (v, *f)))
            .collect::<Option<Vec<_>>>()?,
        rows: c.rows,
        deleted: c.deleted,
        slots: c.slots,
        created: c.created,
        updated: c.updated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_come_back_as_they_went() {
        let d = Date::new(2026, 2, 28).unwrap();
        let t = Time::new(23, 59, 1).unwrap();
        let vals = [
            Value::Null,
            Value::Num(Decimal::parse("-12.50").unwrap()),
            Value::Str("ab c".into()),
            Value::Double(1.5e-7),
            Value::Real(2.25),
            Value::DecFloat(DecFloat::parse("1.0E+3", DfFormat::D34).unwrap()),
            Value::Date(d),
            Value::Time(t),
            Value::Timestamp(Timestamp {
                date: d,
                time: t,
                picos: 123_456_789_012,
                precision: 12,
            }),
            Value::Bin(vec![0, 255, 7]),
            Value::Bool(true),
        ];
        for v in vals {
            let back = decode_value(&encode_value(&v).unwrap()).unwrap();
            assert_eq!(
                back.compare(&v).ok().flatten(),
                if v.is_null() {
                    None
                } else {
                    Some(std::cmp::Ordering::Equal)
                },
                "{v:?}"
            );
            assert_eq!(std::mem::discriminant(&back), std::mem::discriminant(&v));
        }
    }
}
