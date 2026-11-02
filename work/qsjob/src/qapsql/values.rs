// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database's types and values as QAP sends them (docs/QAP.md,
//! Service 5): a `DESCRIBE` entry (SQLTYPE, SQLLEN, name, CCSID) as a
//! `Column`, and a value of the engine's as an `SqlValue` by its
//! column, and back for parameters. Integers are `Integer`, `DECIMAL`
//! and `NUMERIC` exact `Decimal` text, `REAL` and `DOUBLE` the shortest
//! `Float` text, character and graphic data `Text` but `Binary` with
//! CCSID 65535 (its CP037 bytes as stored), dates, times and
//! timestamps milliseconds (timestamps to the microsecond, scale 6).

use qap_proto::ops::sql::{Column, SqlValue};
use sql::embed::{from_wire, to_wire};
use sql::value::Value;

/// A column's type as the engine describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `SMALLINT`, `INTEGER`, `BIGINT`.
    Int,
    /// `DECIMAL`, `NUMERIC`.
    Dec,
    /// `REAL`.
    Real,
    /// `DOUBLE`.
    Double,
    /// Character or graphic.
    Text,
    /// Character with CCSID 65535.
    Bits,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP`.
    Timestamp,
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
        480 if len == 4 => ("REAL", Kind::Real, 24, 0, 0),
        480 => ("DOUBLE", Kind::Double, 53, 0, 0),
        384 => ("DATE", Kind::Date, 10, 0, 0),
        388 => ("TIME", Kind::Time, 8, 0, 0),
        392 => ("TIMESTAMP", Kind::Timestamp, 26, 6, 0),
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

/// `yyyy-mm-dd hh:mm:ss.ffffff` (or `-` and `.`) as milliseconds and
/// the picoseconds past them.
fn timestamp_of(text: &str) -> Option<(i64, u32)> {
    let t = text.trim();
    let ms = date_of(t.get(..10)?)? + time_of(t.get(11..19)?)?;
    let frac = t.get(20..).unwrap_or("");
    let digits = format!("{frac:0<12}");
    let ps: u64 = digits.get(..12)?.parse().ok()?;
    Some((ms + i64::try_from(ps / 1_000_000_000).ok()?, u32::try_from(ps % 1_000_000_000).ok()?))
}

/// The shortest of the plain and the E notation of a number's text.
fn shortest(plain: String, exp: String) -> String {
    if exp.len() < plain.len() { exp } else { plain }
}

/// A value of a column of `kind`, as the client is sent it.
#[must_use]
pub fn to_sql(kind: Kind, v: &Value) -> SqlValue {
    if *v == Value::Null {
        return SqlValue::Null;
    }
    let (_, text) = to_wire(v);
    match kind {
        Kind::Int => match v.as_i64() {
            Ok(value) => SqlValue::Integer { value },
            Err(_) => SqlValue::Decimal { text },
        },
        Kind::Dec => SqlValue::Decimal { text },
        #[allow(clippy::cast_possible_truncation)] // a REAL's value
        Kind::Real => match text.parse::<f64>() {
            Ok(f) => {
                let f = f as f32;
                SqlValue::Float {
                    text: shortest(format!("{f}"), format!("{f:e}")),
                }
            }
            Err(_) => SqlValue::Float { text },
        },
        Kind::Double => match text.parse::<f64>() {
            Ok(f) => SqlValue::Float {
                text: shortest(format!("{f}"), format!("{f:e}")),
            },
            Err(_) => SqlValue::Float { text },
        },
        Kind::Text => SqlValue::Text { text },
        // CP037 maps every byte, so the bytes come back as stored.
        Kind::Bits => SqlValue::Binary {
            data: text.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect(),
        },
        Kind::Date => date_of(&text).map_or(SqlValue::Text { text }, |ms| SqlValue::Date { ms }),
        Kind::Time => time_of(&text).map_or(SqlValue::Text { text }, |ms| SqlValue::Time { ms }),
        Kind::Timestamp => timestamp_of(&text).map_or(SqlValue::Text { text }, |(ms, picos)| SqlValue::Timestamp { ms, picos }),
    }
}

/// A parameter for a column of `kind`; `None` when the value cannot be
/// one of the engine's (the engine has no `DECFLOAT` special values,
/// no binary types and timestamps to the microsecond only).
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
        SqlValue::Decimal { text } | SqlValue::Float { text } => {
            let t = text.trim();
            match qdecimal::Decimal::parse(t) {
                Ok(d) => Value::Num(d),
                // E notation, as a double reads it.
                Err(_) => Value::Num(qdecimal::Decimal::parse(&format!("{:.17}", t.parse::<f64>().ok().filter(|f| f.is_finite())?)).ok()?),
            }
        }
        SqlValue::Text { text } => match kind {
            Kind::Date | Kind::Time | Kind::Timestamp => from_wire(&(tag(kind), text.clone())),
            _ => Value::Str(text.clone()),
        },
        SqlValue::Binary { data } => Value::Str(ccsid::from_ebcdic(data)),
        SqlValue::Date { ms } => from_wire(&(3, ms_date(*ms))),
        SqlValue::Time { ms } => from_wire(&(4, ms_time(*ms))),
        SqlValue::Timestamp { ms, picos } => {
            let micros = ms.rem_euclid(1000) * 1000 + i64::from(*picos / 1_000_000);
            let t = format!("{} {}.{micros:06}", ms_date(*ms), ms_time(*ms).replace('.', ":"));
            from_wire(&(5, t))
        }
        SqlValue::Boolean { value } => Value::int(i64::from(*value)),
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
        assert_eq!(to_sql(Kind::Timestamp, &from_sql(Kind::Timestamp, &ts).unwrap()), ts);
        // Past the microsecond the engine keeps nothing.
        let fine = SqlValue::Timestamp { ms: ms + 1, picos: 456_789 };
        assert_eq!(to_sql(Kind::Timestamp, &from_sql(Kind::Timestamp, &fine).unwrap()), SqlValue::Timestamp { ms: ms + 1, picos: 0 });
    }

    #[test]
    fn numbers_and_text() {
        let d = from_sql(Kind::Dec, &SqlValue::Decimal { text: "-12.50".into() }).unwrap();
        assert_eq!(to_sql(Kind::Dec, &d), SqlValue::Decimal { text: "-12.50".into() });
        assert_eq!(to_sql(Kind::Int, &Value::int(7)), SqlValue::Integer { value: 7 });
        let f = from_sql(Kind::Double, &SqlValue::Float { text: "0.1".into() }).unwrap();
        assert_eq!(to_sql(Kind::Double, &f), SqlValue::Float { text: "0.1".into() });
        assert_eq!(from_sql(Kind::Dec, &SqlValue::Decimal { text: "NaN".into() }), None);
        let bits = SqlValue::Binary { data: vec![0x00, 0xC1, 0xFF, 0x40] };
        assert_eq!(to_sql(Kind::Bits, &from_sql(Kind::Bits, &bits).unwrap()), bits);
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
    }
}
