// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database's types and values as QAP sends them (docs/QAP.md,
//! Service 5): a `DESCRIBE` entry (SQLTYPE, SQLLEN, name, CCSID) as a
//! `Column`, and a value of the engine's as an `SqlValue` by its
//! column, and back for parameters. Integers are `Integer`, `DECIMAL`
//! and `NUMERIC` exact `Decimal` text, `DECFLOAT` `Decimal` text with E
//! notation and the special values, `REAL` and `DOUBLE` the shortest
//! `Float` text, character, graphic and character LOB data `Text`;
//! `BINARY`, `VARBINARY`, `BLOB` and character data with CCSID 65535
//! (its CP037 bytes as stored) `Binary`; `BOOLEAN` `Boolean`; dates,
//! times and timestamps milliseconds, timestamps with the picoseconds
//! past them and the column's precision as its scale.

use qap_proto::ops::sql::{Column, SqlValue};
use qdecimal::{DecFloat, DfFormat};
use sql::embed::{from_wire, to_wire};
use sql::value::{PICOS, Timestamp, Value};

/// A column's type as the engine describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `SMALLINT`, `INTEGER`, `BIGINT`.
    Int,
    /// `DECIMAL`, `NUMERIC`.
    Dec,
    /// `DECFLOAT`.
    DecFloat,
    /// `REAL`.
    Real,
    /// `DOUBLE`.
    Double,
    /// Character, graphic, `CLOB`, `DBCLOB`.
    Text,
    /// Character with CCSID 65535.
    Bits,
    /// `BINARY`, `VARBINARY`, `BLOB`.
    Binary,
    /// `BOOLEAN`.
    Boolean,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP(p)`.
    Timestamp(u8),
}

/// A described column: what the client is told, and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Described {
    /// The description.
    pub column: Column,
    /// The kind.
    pub kind: Kind,
}

/// The job's CCSID for character data the engine gives none.
const JOB_CCSID: u16 = 37;
/// UTF-16, for graphic data the engine gives none.
const GRAPHIC_CCSID: u16 = 1200;

/// One `DESCRIBE` row (SQLTYPE, SQLLEN, SQLNAME, CCSID, label) as a
/// column; without a label, the name is the label.
#[must_use]
pub fn described(row: &[Value]) -> Described {
    let num = |i: usize| row.get(i).and_then(|v| v.as_i64().ok()).unwrap_or(0);
    let code = num(0);
    let len = u32::try_from(num(1)).unwrap_or(0);
    let name = row.get(2).map(Value::as_text).unwrap_or_default().trim().to_string();
    let ccsid = u16::try_from(num(3)).unwrap_or(0);
    let label = row.get(4).map(Value::as_text).unwrap_or_default().trim_end().to_string();
    let label = if label.is_empty() { name.clone() } else { label };
    let base = code & !1;
    let (sql_type, kind, length, scale, ccsid) = match base {
        500 => ("SMALLINT", Kind::Int, 5, 0, 0),
        496 => ("INTEGER", Kind::Int, 10, 0, 0),
        492 => ("BIGINT", Kind::Int, 19, 0, 0),
        484 => ("DECIMAL", Kind::Dec, len >> 8, len & 0xFF, 0),
        488 => ("NUMERIC", Kind::Dec, len >> 8, len & 0xFF, 0),
        // SQLLEN is the bytes; the column's length its digits.
        996 => ("DECFLOAT", Kind::DecFloat, if len == 8 { 16 } else { 34 }, 0, 0),
        480 if len == 4 => ("REAL", Kind::Real, 24, 0, 0),
        480 => ("DOUBLE", Kind::Double, 53, 0, 0),
        384 => ("DATE", Kind::Date, 10, 0, 0),
        388 => ("TIME", Kind::Time, 8, 0, 0),
        392 => {
            // 19 long is TIMESTAMP(0), else 20 + p.
            let p = if len <= 19 { 0 } else { (len - 20).min(12) };
            ("TIMESTAMP", Kind::Timestamp(u8::try_from(p).expect("small")), len.max(19), p, 0)
        }
        912 => ("BINARY", Kind::Binary, len, 0, 65535),
        908 => ("VARBINARY", Kind::Binary, len, 0, 65535),
        404 => ("BLOB", Kind::Binary, len, 0, 65535),
        408 => ("CLOB", Kind::Text, len, 0, if ccsid == 0 { JOB_CCSID } else { ccsid }),
        412 => ("DBCLOB", Kind::Text, len, 0, if ccsid == 0 { GRAPHIC_CCSID } else { ccsid }),
        2436 => ("BOOLEAN", Kind::Boolean, 1, 0, 0),
        452 | 448 | 468 | 464 => {
            let t = match base {
                452 => "CHAR",
                448 => "VARCHAR",
                468 => "GRAPHIC",
                _ => "VARGRAPHIC",
            };
            let default = if matches!(base, 468 | 464) { GRAPHIC_CCSID } else { JOB_CCSID };
            let ccsid = if ccsid == 0 { default } else { ccsid };
            let kind = if ccsid == 65535 { Kind::Bits } else { Kind::Text };
            (t, kind, len, 0, ccsid)
        }
        _ => ("VARCHAR", Kind::Text, len, 0, JOB_CCSID),
    };
    Described {
        column: Column {
            label,
            name,
            sql_type: sql_type.into(),
            length,
            scale: u16::try_from(scale).unwrap_or(0),
            nullable: code & 1 == 1,
            ccsid,
        },
        kind,
    }
}

/// Midnight UTC of a date, in milliseconds since 1970-01-01.
fn date_ms(y: i64, m: i64, d: i64) -> i64 {
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400_000
}

/// The civil date of a day count since 1970-01-01.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Numbers in `text` split at any of `seps`.
fn numbers(text: &str, seps: &[char]) -> Option<Vec<i64>> {
    text.trim().split(seps).map(|p| p.parse().ok()).collect()
}

/// `yyyy-mm-dd` as milliseconds.
fn date_of(text: &str) -> Option<i64> {
    let v = numbers(text, &['-'])?;
    let [y, m, d] = v[..] else { return None };
    Some(date_ms(y, m, d))
}

/// `hh.mm.ss` or `hh:mm:ss` as milliseconds since midnight.
fn time_of(text: &str) -> Option<i64> {
    let v = numbers(text, &['.', ':'])?;
    let [h, m, s] = v[..] else { return None };
    Some(((h * 60 + m) * 60 + s) * 1000)
}

/// A timestamp as milliseconds and the picoseconds past them, digits
/// past `p` dropped.
fn timestamp_ms(t: &Timestamp, p: u8) -> (i64, u32) {
    let t = t.with_precision(p);
    let day = date_ms(i64::from(t.date.year), i64::from(t.date.month), i64::from(t.date.day));
    let secs = (i64::from(t.time.hour) * 60 + i64::from(t.time.minute)) * 60 + i64::from(t.time.second);
    let ms = day + secs * 1000 + i64::try_from(t.picos / 1_000_000_000).expect("below 1000");
    (ms, u32::try_from(t.picos % 1_000_000_000).expect("below 10^9"))
}

/// The shortest of the plain and the E notation of a number's text.
fn shortest(plain: String, exp: String) -> String {
    if exp.len() < plain.len() { exp } else { plain }
}

/// A `DECFLOAT`'s text as QAP spells it: the engine's, `sNaN` for a
/// signaling NaN.
fn decfloat_text(d: &DecFloat) -> String {
    d.to_string().replace("SNaN", "sNaN")
}

/// A value of a column of `kind`, as the client is sent it.
#[must_use]
pub fn to_sql(kind: Kind, v: &Value) -> SqlValue {
    match (kind, v) {
        (_, Value::Null) => return SqlValue::Null,
        (_, Value::DecFloat(d)) => return SqlValue::Decimal { text: decfloat_text(d) },
        (_, Value::Real(f)) => return SqlValue::Float { text: shortest(format!("{f}"), format!("{f:e}")) },
        (_, Value::Double(f)) => return SqlValue::Float { text: shortest(format!("{f}"), format!("{f:e}")) },
        (Kind::Binary | Kind::Text | Kind::Bits, Value::Bin(data)) => return SqlValue::Binary { data: data.clone() },
        (_, Value::Bool(value)) => return SqlValue::Boolean { value: *value },
        (Kind::Boolean, v) => {
            if let Ok(value) = v.as_bool() {
                return SqlValue::Boolean { value };
            }
        }
        (Kind::Timestamp(p), Value::Timestamp(t)) => {
            let (ms, picos) = timestamp_ms(t, p);
            return SqlValue::Timestamp { ms, picos };
        }
        _ => {}
    }
    let (_, text) = to_wire(v);
    match kind {
        Kind::Int => match v.as_i64() {
            Ok(value) => SqlValue::Integer { value },
            Err(_) => SqlValue::Decimal { text },
        },
        Kind::Dec | Kind::DecFloat => SqlValue::Decimal { text },
        #[allow(clippy::cast_possible_truncation)] // a REAL's value
        Kind::Real => match text.parse::<f64>() {
            Ok(f) => {
                let f = f as f32;
                SqlValue::Float { text: shortest(format!("{f}"), format!("{f:e}")) }
            }
            Err(_) => SqlValue::Float { text },
        },
        Kind::Double => match text.parse::<f64>() {
            Ok(f) => SqlValue::Float { text: shortest(format!("{f}"), format!("{f:e}")) },
            Err(_) => SqlValue::Float { text },
        },
        Kind::Text | Kind::Boolean => SqlValue::Text { text },
        // CP037 maps every byte, so the bytes come back as stored.
        Kind::Bits | Kind::Binary => {
            SqlValue::Binary { data: text.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect() }
        }
        Kind::Date => date_of(&text).map_or(SqlValue::Text { text }, |ms| SqlValue::Date { ms }),
        Kind::Time => time_of(&text).map_or(SqlValue::Text { text }, |ms| SqlValue::Time { ms }),
        Kind::Timestamp(p) => match Timestamp::parse(&text) {
            Some(t) => {
                let (ms, picos) = timestamp_ms(&t, p);
                SqlValue::Timestamp { ms, picos }
            }
            None => SqlValue::Text { text },
        },
    }
}

/// A parameter for a column of `kind`; `None` when the value cannot be
/// one of the engine's (digits past 63, an E notation past a double
/// for a column that is not `DECFLOAT`, special values outside one).
#[must_use]
pub fn from_sql(kind: Kind, v: &SqlValue) -> Option<Value> {
    let ms_date = |ms: i64| {
        let (y, m, d) = civil(ms.div_euclid(86_400_000));
        format!("{y:04}-{m:02}-{d:02}")
    };
    let ms_time = |ms: i64| {
        let s = ms.rem_euclid(86_400_000) / 1000;
        format!("{:02}.{:02}.{:02}", s / 3600, s / 60 % 60, s % 60)
    };
    Some(match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer { value } => Value::int(*value),
        SqlValue::Decimal { text } if kind == Kind::DecFloat => {
            Value::DecFloat(DecFloat::parse(text, DfFormat::D34).ok()?)
        }
        // A float as the double it reads as.
        SqlValue::Float { text } => Value::Double(text.trim().parse::<f64>().ok()?),
        SqlValue::Decimal { text } => {
            let t = text.trim();
            match qdecimal::Decimal::parse(t) {
                Ok(d) => Value::Num(d),
                // E notation: a double.
                Err(_) => Value::Double(t.parse::<f64>().ok().filter(|f| f.is_finite())?),
            }
        }
        SqlValue::Text { text } => match kind {
            Kind::Date | Kind::Time | Kind::Timestamp(_) => from_wire(&(tag(kind), text.clone())),
            _ => Value::Str(text.clone()),
        },
        SqlValue::Binary { data } if kind == Kind::Bits => Value::Str(ccsid::from_ebcdic(data)),
        SqlValue::Binary { data } => Value::Bin(data.clone()),
        SqlValue::Date { ms } => from_wire(&(3, ms_date(*ms))),
        SqlValue::Time { ms } => from_wire(&(4, ms_time(*ms))),
        SqlValue::Timestamp { ms, picos } => {
            // Every digit: the column keeps those of its precision.
            let fraction = u64::try_from(ms.rem_euclid(1000)).expect("below 1000") * 1_000_000_000 + u64::from(*picos);
            let t = format!("{} {}.{:012}", ms_date(*ms), ms_time(*ms).replace('.', ":"), fraction.min(PICOS - 1));
            from_wire(&(5, t))
        }
        SqlValue::Boolean { value } => Value::Bool(*value),
    })
}

/// The wire tag of a date, time or timestamp.
fn tag(kind: Kind) -> u8 {
    match kind {
        Kind::Date => 3,
        Kind::Time => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_both_ways() {
        assert_eq!(date_ms(1970, 1, 1), 0);
        assert_eq!(civil(0), (1970, 1, 1));
        let ms = date_ms(2026, 11, 2);
        assert_eq!(civil(ms / 86_400_000), (2026, 11, 2));
        assert_eq!(civil(date_ms(2000, 2, 29) / 86_400_000), (2000, 2, 29));
        let v = from_sql(Kind::Date, &SqlValue::Date { ms }).unwrap();
        assert_eq!(to_sql(Kind::Date, &v), SqlValue::Date { ms });
        let t = SqlValue::Time { ms: 33_243_000 };
        assert_eq!(to_sql(Kind::Time, &from_sql(Kind::Time, &t).unwrap()), t);
        let ts = SqlValue::Timestamp { ms: ms + 33_243_123, picos: 456_000_000 };
        assert_eq!(to_sql(Kind::Timestamp(6), &from_sql(Kind::Timestamp(6), &ts).unwrap()), ts);
        // TIMESTAMP(12) keeps every picosecond; TIMESTAMP(6) drops those past the microsecond.
        let fine = SqlValue::Timestamp { ms: ms + 1, picos: 456_789 };
        assert_eq!(to_sql(Kind::Timestamp(12), &from_sql(Kind::Timestamp(12), &fine).unwrap()), fine);
        let six = to_sql(Kind::Timestamp(6), &from_sql(Kind::Timestamp(6), &fine).unwrap());
        assert_eq!(six, SqlValue::Timestamp { ms: ms + 1, picos: 0 });
        // A value as the engine gives it.
        let engine = Value::Timestamp(Timestamp::parse("1970-01-01-00.00.01.123456789012").unwrap());
        assert_eq!(to_sql(Kind::Timestamp(12), &engine), SqlValue::Timestamp { ms: 1123, picos: 456_789_012 });
        assert_eq!(to_sql(Kind::Timestamp(0), &engine), SqlValue::Timestamp { ms: 1000, picos: 0 });
    }

    #[test]
    fn numbers_and_text() {
        let d = from_sql(Kind::Dec, &SqlValue::Decimal { text: "-12.50".into() }).unwrap();
        assert_eq!(to_sql(Kind::Dec, &d), SqlValue::Decimal { text: "-12.50".into() });
        assert_eq!(to_sql(Kind::Int, &Value::int(7)), SqlValue::Integer { value: 7 });
        let f = from_sql(Kind::Double, &SqlValue::Float { text: "0.1".into() }).unwrap();
        assert_eq!(to_sql(Kind::Double, &f), SqlValue::Float { text: "0.1".into() });
        // Past a decimal's 63 digits, as IEEE holds them.
        for text in ["1e300", "-2.5e-308", "1e-45"] {
            let f = from_sql(Kind::Double, &SqlValue::Float { text: text.into() }).unwrap();
            assert_eq!(to_sql(Kind::Double, &f), SqlValue::Float { text: text.into() });
        }
        assert_eq!(to_sql(Kind::Real, &Value::Real(3.402_823_5e38)), SqlValue::Float { text: "3.4028235e38".into() });
        assert_eq!(from_sql(Kind::Dec, &SqlValue::Decimal { text: "NaN".into() }), None);
        let bits = SqlValue::Binary { data: vec![0x00, 0xC1, 0xFF, 0x40] };
        assert_eq!(to_sql(Kind::Bits, &from_sql(Kind::Bits, &bits).unwrap()), bits);
    }

    #[test]
    fn decfloat_binary_and_boolean() {
        for text in ["1.5E-30", "-12.50", "Infinity", "-Infinity", "NaN", "sNaN", "1E+400"] {
            let v = from_sql(Kind::DecFloat, &SqlValue::Decimal { text: text.into() }).unwrap();
            assert_eq!(to_sql(Kind::DecFloat, &v), SqlValue::Decimal { text: text.into() }, "{text}");
        }
        let data = SqlValue::Binary { data: vec![0, 1, 0xFF] };
        let v = from_sql(Kind::Binary, &data).unwrap();
        assert_eq!(v, Value::Bin(vec![0, 1, 0xFF]));
        assert_eq!(to_sql(Kind::Binary, &v), data);
        let t = SqlValue::Boolean { value: true };
        assert_eq!(to_sql(Kind::Boolean, &from_sql(Kind::Boolean, &t).unwrap()), t);
        assert_eq!(to_sql(Kind::Text, &Value::Str("a CLOB".into())), SqlValue::Text { text: "a CLOB".into() });
    }

    #[test]
    fn describe_rows() {
        let row = |t: i64, l: i64, n: &str, c: i64| vec![Value::int(t), Value::int(l), Value::Str(n.into()), Value::int(c)];
        let d = described(&row(485, 11 * 256 + 2, "COST", 0));
        assert_eq!((d.column.sql_type.as_str(), d.column.length, d.column.scale, d.column.nullable), ("DECIMAL", 11, 2, true));
        let b = described(&row(452, 8, "BITS", 65535));
        assert_eq!((b.kind, b.column.ccsid), (Kind::Bits, 65535));
        assert_eq!(described(&row(448, 40, "NAME", 0)).column.ccsid, 37);
        assert_eq!(described(&row(480, 4, "R", 0)).column.sql_type, "REAL");
        assert_eq!(described(&row(392, 26, "TS", 0)).column.scale, 6);
        let ts = described(&row(393, 32, "TS12", 0));
        assert_eq!((ts.kind, ts.column.scale, ts.column.length), (Kind::Timestamp(12), 12, 32));
        assert_eq!(described(&row(392, 19, "TS0", 0)).column.scale, 0);
        let df = described(&row(997, 8, "D", 0));
        assert_eq!((df.kind, df.column.sql_type.as_str(), df.column.length), (Kind::DecFloat, "DECFLOAT", 16));
        assert_eq!(described(&row(996, 16, "D", 0)).column.length, 34);
        let vb = described(&row(908, 100, "VB", 65535));
        assert_eq!((vb.kind, vb.column.sql_type.as_str(), vb.column.ccsid), (Kind::Binary, "VARBINARY", 65535));
        assert_eq!(described(&row(404, 1 << 20, "B", 65535)).column.sql_type, "BLOB");
        let c = described(&row(408, 1024, "C", 0));
        assert_eq!((c.kind, c.column.ccsid), (Kind::Text, 37));
        assert_eq!(described(&row(412, 10, "G", 1200)).column.sql_type, "DBCLOB");
        assert_eq!(described(&row(2437, 1, "F", 0)).kind, Kind::Boolean);
    }
}
