// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test database's column types and how a value becomes one: the
//! assignment rules a database applies (padding, scale, range, the
//! timestamp's precision), and the failures it reports when a value
//! does not fit. `DECFLOAT` is the engine's own (`qdecimal`), so its
//! exponent and text are what QS/400 gives.

use qap_proto::ops::sql::{Column, SqlValue};
use qdecimal::{DecFloat, DecimalError, DfFormat};

use super::parse::Lit;

/// A column's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `SMALLINT`, `INTEGER`, `BIGINT`: the range.
    Int(i64, i64),
    /// `DECIMAL` or `NUMERIC`: precision and scale.
    Dec(u32, u32),
    /// `DECFLOAT`.
    DecFloat,
    /// `REAL`.
    Real,
    /// `DOUBLE`.
    Double,
    /// Character or graphic: the length, fixed (padded) or varying.
    Text(u32, bool),
    /// Character with CCSID 65535: bytes, the length, fixed or not.
    Bits(u32, bool),
    /// `BINARY`, `VARBINARY`, `BLOB`: the length, fixed or not.
    Bin(u32, bool),
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP(p)`.
    Ts(u16),
    /// `BOOLEAN`.
    Bool,
}

/// A column: what `Prepare` and `ListColumns` describe, and its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Col {
    /// The description.
    pub column: Column,
    /// The type.
    pub kind: Kind,
}

/// Make a column of SQL type `sql_type`: `CHAR(10)`, `DECIMAL(11,2)`,
/// `CHAR(8) FOR BIT DATA`, `TIMESTAMP(12)`, ...
///
/// # Panics
/// On a type the test database does not have (a fixture's mistake).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn col(name: &str, label: &str, sql_type: &str, nullable: bool) -> Col {
    let (base, args) = match sql_type.split_once('(') {
        Some((b, rest)) => {
            let (a, tail) = rest.split_once(')').expect("closed");
            (
                format!("{b}{tail}"),
                a.split(',')
                    .map(|x| x.trim().parse::<u32>().expect("a number"))
                    .collect(),
            )
        }
        None => (sql_type.to_string(), Vec::<u32>::new()),
    };
    let arg = |i: usize, d: u32| args.get(i).copied().unwrap_or(d);
    let (t, kind, length, scale, ccsid) = match base.as_str() {
        "SMALLINT" => (
            "SMALLINT",
            Kind::Int(i64::from(i16::MIN), i64::from(i16::MAX)),
            5,
            0,
            0,
        ),
        "INTEGER" => (
            "INTEGER",
            Kind::Int(i64::from(i32::MIN), i64::from(i32::MAX)),
            10,
            0,
            0,
        ),
        "BIGINT" => ("BIGINT", Kind::Int(i64::MIN, i64::MAX), 19, 0, 0),
        "DECIMAL" => (
            "DECIMAL",
            Kind::Dec(arg(0, 5), arg(1, 0)),
            arg(0, 5),
            arg(1, 0),
            0,
        ),
        "NUMERIC" => (
            "NUMERIC",
            Kind::Dec(arg(0, 5), arg(1, 0)),
            arg(0, 5),
            arg(1, 0),
            0,
        ),
        "DECFLOAT" => ("DECFLOAT", Kind::DecFloat, arg(0, 34), 0, 0),
        "REAL" => ("REAL", Kind::Real, 24, 0, 0),
        "DOUBLE" => ("DOUBLE", Kind::Double, 53, 0, 0),
        "CHAR" => ("CHAR", Kind::Text(arg(0, 1), true), arg(0, 1), 0, 37),
        "VARCHAR" => ("VARCHAR", Kind::Text(arg(0, 1), false), arg(0, 1), 0, 37),
        "CLOB" => (
            "CLOB",
            Kind::Text(arg(0, 1_048_576), false),
            arg(0, 1_048_576),
            0,
            37,
        ),
        "GRAPHIC" => ("GRAPHIC", Kind::Text(arg(0, 1), true), arg(0, 1), 0, 1200),
        "VARGRAPHIC" => (
            "VARGRAPHIC",
            Kind::Text(arg(0, 1), false),
            arg(0, 1),
            0,
            1200,
        ),
        "CHAR FOR BIT DATA" => ("CHAR", Kind::Bits(arg(0, 1), true), arg(0, 1), 0, 65535),
        "VARCHAR FOR BIT DATA" => ("VARCHAR", Kind::Bits(arg(0, 1), false), arg(0, 1), 0, 65535),
        "BINARY" => ("BINARY", Kind::Bin(arg(0, 1), true), arg(0, 1), 0, 0),
        "VARBINARY" => ("VARBINARY", Kind::Bin(arg(0, 1), false), arg(0, 1), 0, 0),
        "BLOB" => (
            "BLOB",
            Kind::Bin(arg(0, 1_048_576), false),
            arg(0, 1_048_576),
            0,
            0,
        ),
        "DATE" => ("DATE", Kind::Date, 10, 0, 0),
        "TIME" => ("TIME", Kind::Time, 8, 0, 0),
        "TIMESTAMP" => {
            let p = arg(0, 6);
            (
                "TIMESTAMP",
                Kind::Ts(u16::try_from(p).unwrap_or(6)),
                20 + p,
                p,
                0,
            )
        }
        "BOOLEAN" => ("BOOLEAN", Kind::Bool, 1, 0, 0),
        other => panic!("no type {other}"),
    };
    Col {
        column: Column {
            name: name.into(),
            label: label.into(),
            sql_type: t.into(),
            length,
            scale: u16::try_from(scale).unwrap_or(0),
            nullable,
            ccsid: u16::try_from(ccsid).unwrap_or(0),
        },
        kind,
    }
}

/// Why a value does not fit: SQLSTATE, SQLCODE and the message.
pub type Misfit = (&'static str, i32, &'static str);

const WRONG_TYPE: Misfit = ("07006", -301, "SQL0301");
const TOO_LONG: Misfit = ("22001", -404, "SQL0404");
const OUT_OF_RANGE: Misfit = ("22003", -406, "SQL0406");
const NOT_A_NUMBER: Misfit = ("22018", -420, "SQL0420");
const NULL_NOT_ALLOWED: Misfit = ("23502", -407, "SQL0407");

/// Midnight UTC of a date, in milliseconds since 1970-01-01.
#[must_use]
pub fn date_ms(y: i64, m: u32, d: u32) -> i64 {
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400_000
}

/// `yyyy-mm-dd`.
fn parse_date(text: &str) -> Option<i64> {
    let mut parts = text.trim().split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    ((1..=12).contains(&month) && (1..=31).contains(&day)).then(|| date_ms(year, month, day))
}

/// `hh:mm:ss` or `hh.mm.ss`.
fn parse_time(s: &str) -> Option<i64> {
    let v: Vec<i64> = s
        .trim()
        .split([':', '.'])
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    let [h, m, sec] = v[..] else { return None };
    (h < 24 && m < 60 && sec < 60).then_some(((h * 60 + m) * 60 + sec) * 1000)
}

/// `yyyy-mm-dd-hh.mm.ss[.f...]` or `yyyy-mm-dd hh:mm:ss[.f...]`: the
/// milliseconds and the picoseconds past them.
#[must_use]
pub fn parse_timestamp(s: &str) -> Option<(i64, u32)> {
    let s = s.trim();
    let date = parse_date(s.get(..10)?)?;
    let time = parse_time(s.get(11..19)?)?;
    let frac = s.get(20..).unwrap_or("");
    if !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 12 {
        return None;
    }
    let digits = format!("{frac:0<12}");
    let ps: u64 = digits.parse().ok()?;
    let ms = date + time + i64::try_from(ps / 1_000_000_000).ok()?;
    Some((ms, u32::try_from(ps % 1_000_000_000).ok()?))
}

/// A literal as the value it reads as before assignment.
#[must_use]
pub fn literal(l: &Lit) -> SqlValue {
    match l {
        Lit::Null => SqlValue::Null,
        Lit::Num(n) => match n.parse::<i64>() {
            Ok(v) if !n.contains('.') => SqlValue::Integer { value: v },
            _ => SqlValue::Decimal { text: n.clone() },
        },
        Lit::Str(s) => SqlValue::Text { text: s.clone() },
    }
}

/// Whether `t` is a number a decimal type can take.
fn decimal_parts(t: &str) -> Option<(bool, String, String)> {
    let t = t.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (i, f) = t.split_once('.').unwrap_or((t, ""));
    if (i.is_empty() && f.is_empty()) || !i.chars().chain(f.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let i = i.trim_start_matches('0');
    Some((
        neg,
        if i.is_empty() { "0".into() } else { i.into() },
        f.into(),
    ))
}

/// The shortest text that reads back as `v`.
#[must_use]
pub fn float_text(v: f64) -> String {
    let plain = format!("{v}");
    let exp = format!("{v:e}");
    if exp.len() < plain.len() { exp } else { plain }
}

fn real_text(v: f32) -> String {
    let plain = format!("{v}");
    let exp = format!("{v:e}");
    if exp.len() < plain.len() { exp } else { plain }
}

/// `v` as a `DECFLOAT` of column `c`, by the engine's own rules: a
/// `Decimal` (and a literal) read as `DECFLOAT(34)` from its text, its
/// exponent kept; an integer exact; a float its shortest digits; text
/// parsed; then rounded to the column's 16 or 34 digits.
fn decfloat(c: &Column, v: &SqlValue) -> Result<DecFloat, Misfit> {
    let format = DfFormat::of_digits(c.length).unwrap_or(DfFormat::D34);
    let misfit = |e: DecimalError| {
        if matches!(e, DecimalError::Parse { .. }) {
            NOT_A_NUMBER
        } else {
            OUT_OF_RANGE
        }
    };
    let d = match v {
        SqlValue::Integer { value } => Ok(DecFloat::from_i64(*value, DfFormat::D34)),
        SqlValue::Decimal { text } => DecFloat::parse(text, DfFormat::D34),
        SqlValue::Float { text } => {
            let f: f64 = text.trim().parse().map_err(|_| NOT_A_NUMBER)?;
            DecFloat::from_f64(f, format)
        }
        SqlValue::Text { text } => DecFloat::parse(text, format),
        _ => return Err(WRONG_TYPE),
    };
    d.and_then(|d| d.to_format(format)).map_err(misfit)
}

fn number_text(v: &SqlValue) -> Option<String> {
    match v {
        SqlValue::Integer { value } => Some(value.to_string()),
        SqlValue::Decimal { text } | SqlValue::Float { text } => Some(text.trim().to_string()),
        _ => None,
    }
}

/// `v` assigned to a column of `c`: the value as stored, or why not.
///
/// # Errors
/// [`Misfit`]: the value's type, length or range does not fit.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)] // one arm per type
pub fn assign(c: &Col, v: SqlValue) -> Result<SqlValue, Misfit> {
    if v == SqlValue::Null {
        return if c.column.nullable {
            Ok(v)
        } else {
            Err(NULL_NOT_ALLOWED)
        };
    }
    Ok(match (c.kind, v) {
        (Kind::Int(lo, hi), v) => {
            let n = match v {
                SqlValue::Integer { value } => value,
                SqlValue::Decimal { text } => match decimal_parts(&text) {
                    Some((neg, i, f)) if f.trim_end_matches('0').is_empty() => {
                        let n: i64 = i.parse().map_err(|_| OUT_OF_RANGE)?;
                        if neg { -n } else { n }
                    }
                    Some(_) => return Err(OUT_OF_RANGE),
                    None => return Err(NOT_A_NUMBER),
                },
                SqlValue::Boolean { value } => i64::from(value),
                _ => return Err(WRONG_TYPE),
            };
            if !(lo..=hi).contains(&n) {
                return Err(OUT_OF_RANGE);
            }
            SqlValue::Integer { value: n }
        }
        (Kind::Dec(p, s), v) => {
            let t = number_text(&v).ok_or(WRONG_TYPE)?;
            let (neg, i, mut f) = decimal_parts(&t).ok_or(NOT_A_NUMBER)?;
            let s = usize::try_from(s).unwrap_or(0);
            if i.len() > usize::try_from(p).unwrap_or(0).saturating_sub(s) && i != "0" {
                return Err(OUT_OF_RANGE);
            }
            f.truncate(s);
            while f.len() < s {
                f.push('0');
            }
            let zero = i == "0" && f.chars().all(|c| c == '0');
            let sign = if neg && !zero { "-" } else { "" };
            let text = if s == 0 {
                format!("{sign}{i}")
            } else {
                format!("{sign}{i}.{f}")
            };
            SqlValue::Decimal { text }
        }
        (Kind::DecFloat, v) => SqlValue::Decimal {
            text: decfloat(&c.column, &v)?.to_sci_string(),
        },
        (Kind::Real | Kind::Double, v) => {
            let t = number_text(&v).ok_or(WRONG_TYPE)?;
            let f: f64 = t.parse().map_err(|_| NOT_A_NUMBER)?;
            #[allow(clippy::cast_possible_truncation)]
            let text = if c.kind == Kind::Real {
                real_text(f as f32)
            } else {
                float_text(f)
            };
            SqlValue::Float { text }
        }
        (Kind::Text(n, fixed), SqlValue::Text { text }) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            if text.trim_end_matches(' ').chars().count() > n {
                return Err(TOO_LONG);
            }
            let mut t: String = text.chars().take(n).collect();
            if fixed {
                while t.chars().count() < n {
                    t.push(' ');
                }
            }
            SqlValue::Text { text: t }
        }
        (Kind::Bits(n, fixed) | Kind::Bin(n, fixed), SqlValue::Binary { mut data }) => {
            let n = usize::try_from(n).unwrap_or(usize::MAX);
            if data.len() > n {
                return Err(TOO_LONG);
            }
            if fixed {
                // An EBCDIC blank pads character data, zero binary.
                let pad = if matches!(c.kind, Kind::Bits(..)) {
                    0x40
                } else {
                    0
                };
                data.resize(n, pad);
            }
            SqlValue::Binary { data }
        }
        (Kind::Date, v @ SqlValue::Date { .. }) | (Kind::Time, v @ SqlValue::Time { .. }) => v,
        (Kind::Date, SqlValue::Text { text }) => SqlValue::Date {
            ms: parse_date(&text).ok_or(NOT_A_NUMBER)?,
        },
        (Kind::Time, SqlValue::Text { text }) => SqlValue::Time {
            ms: parse_time(&text).ok_or(NOT_A_NUMBER)?,
        },
        (Kind::Ts(p), SqlValue::Timestamp { ms, picos }) => truncate(ms, picos, p),
        (Kind::Ts(p), SqlValue::Text { text }) => {
            let (ms, picos) = parse_timestamp(&text).ok_or(NOT_A_NUMBER)?;
            truncate(ms, picos, p)
        }
        (Kind::Bool, SqlValue::Boolean { value }) => SqlValue::Boolean { value },
        (Kind::Bool, SqlValue::Integer { value }) => SqlValue::Boolean { value: value != 0 },
        _ => return Err(WRONG_TYPE),
    })
}

/// A timestamp with the digits past precision `p` zero.
fn truncate(ms: i64, picos: u32, p: u16) -> SqlValue {
    let frac = u64::try_from(ms.rem_euclid(1000)).unwrap_or(0) * 1_000_000_000
        + u64::from(picos.min(qap_proto::ops::sql::MAX_PICOS));
    let unit = 10u64.pow(12 - u32::from(p.min(12)));
    let frac = frac / unit * unit;
    SqlValue::Timestamp {
        ms: ms - ms.rem_euclid(1000) + i64::try_from(frac / 1_000_000_000).unwrap_or(0),
        picos: u32::try_from(frac % 1_000_000_000).unwrap_or(0),
    }
}

/// Whether two stored values of one column are equal, as `=` compares:
/// character data without trailing blanks, numbers by value.
#[must_use]
pub fn same(a: &SqlValue, b: &SqlValue) -> bool {
    match (a, b) {
        (SqlValue::Null, _) | (_, SqlValue::Null) => false,
        (SqlValue::Text { text: x }, SqlValue::Text { text: y }) => x.trim_end() == y.trim_end(),
        (SqlValue::Decimal { text: x }, SqlValue::Decimal { text: y }) => {
            match (decimal_parts(x), decimal_parts(y)) {
                (Some((n1, i1, f1)), Some((n2, i2, f2))) => {
                    let (f1, f2) = (f1.trim_end_matches('0'), f2.trim_end_matches('0'));
                    // A zero's sign does not count (`-0 = 0`).
                    let zero = |i: &str, f: &str| i == "0" && f.is_empty();
                    (n1 && !zero(&i1, f1), i1.as_str(), f1)
                        == (n2 && !zero(&i2, f2), i2.as_str(), f2)
                }
                // A `DECFLOAT`: by value, as the engine compares (`1E+2 = 100`).
                _ => match (
                    DecFloat::parse(x, DfFormat::D34),
                    DecFloat::parse(y, DfFormat::D34),
                ) {
                    (Ok(a), Ok(b)) => a.sql_cmp(&b) == std::cmp::Ordering::Equal,
                    _ => x == y,
                },
            }
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment() {
        let d = col("COST", "", "DECIMAL(11,2)", true);
        assert_eq!(
            assign(
                &d,
                SqlValue::Decimal {
                    text: "12.5".into()
                }
            ),
            Ok(SqlValue::Decimal {
                text: "12.50".into()
            })
        );
        assert_eq!(
            assign(&d, SqlValue::Integer { value: 7 }),
            Ok(SqlValue::Decimal {
                text: "7.00".into()
            })
        );
        assert_eq!(
            assign(
                &d,
                SqlValue::Decimal {
                    text: "1234567890".into()
                }
            ),
            Err(OUT_OF_RANGE)
        );
        let c = col("TAG", "", "CHAR(4)", false);
        assert_eq!(
            assign(&c, SqlValue::Text { text: "AB".into() }),
            Ok(SqlValue::Text {
                text: "AB  ".into()
            })
        );
        assert_eq!(
            assign(
                &c,
                SqlValue::Text {
                    text: "ABCDE".into()
                }
            ),
            Err(TOO_LONG)
        );
        assert_eq!(assign(&c, SqlValue::Null), Err(NULL_NOT_ALLOWED));
        assert_eq!(assign(&c, SqlValue::Integer { value: 1 }), Err(WRONG_TYPE));
        let t = col("TS", "", "TIMESTAMP(6)", true);
        assert_eq!(
            assign(
                &t,
                SqlValue::Timestamp {
                    ms: 1_500,
                    picos: 123_456_789
                }
            ),
            Ok(SqlValue::Timestamp {
                ms: 1_500,
                picos: 123_000_000
            })
        );
        let t0 = col("TS", "", "TIMESTAMP(0)", true);
        assert_eq!(
            assign(
                &t0,
                SqlValue::Timestamp {
                    ms: 1_500,
                    picos: 1
                }
            ),
            Ok(SqlValue::Timestamp {
                ms: 1_000,
                picos: 0
            })
        );
        assert_eq!(
            parse_timestamp("1970-01-01-00.00.01.000000000001"),
            Some((1_000, 1))
        );
        assert_eq!(date_ms(1970, 1, 2), 86_400_000);
        assert_eq!(float_text(1e300), "1e300");
        assert_eq!(float_text(0.1), "0.1");
    }

    #[test]
    fn decfloat_keeps_its_exponent() {
        let dec = |t: &str| SqlValue::Decimal { text: t.into() };
        for ty in ["DECFLOAT(16)", "DECFLOAT(34)"] {
            let c = col("DF", "", ty, true);
            for (input, back) in [
                ("1.5", "1.5"),
                ("1.50", "1.50"),
                ("1.500", "1.500"),
                ("1E+2", "1E+2"),
                ("1e2", "1E+2"),
                ("100", "100"),
                ("1.00E+2", "100"),
                ("0", "0"),
                ("0.00", "0.00"),
                ("0E+3", "0E+3"),
                ("-0", "-0"),
                ("-1.50", "-1.50"),
                ("+0.000001", "0.000001"),
                ("0.0000001", "1E-7"),
                ("sNaN", "sNaN"),
                ("-Infinity", "-Infinity"),
            ] {
                assert_eq!(assign(&c, dec(input)), Ok(dec(back)), "{ty} {input}");
                assert_eq!(
                    assign(&c, SqlValue::Text { text: input.into() }),
                    Ok(dec(back)),
                    "{ty} {input}"
                );
            }
            assert_eq!(assign(&c, SqlValue::Integer { value: -7 }), Ok(dec("-7")));
            // A float has no exponent of its own.
            assert_eq!(
                assign(&c, SqlValue::Float { text: "1e2".into() }),
                Ok(dec("100"))
            );
            assert_eq!(assign(&c, dec("1.2.3")), Err(NOT_A_NUMBER));
            assert_eq!(
                assign(&c, SqlValue::Boolean { value: true }),
                Err(WRONG_TYPE)
            );
            assert!(
                same(&dec("1E+2"), &dec("100"))
                    && same(&dec("1.5"), &dec("1.50"))
                    && !same(&dec("1E+2"), &dec("10"))
            );
            assert!(
                same(&dec("-0"), &dec("0.00"))
                    && same(&dec("0E+3"), &dec("-0"))
                    && !same(&dec("-1.5"), &dec("1.5"))
            );
        }
        let d16 = col("DF", "", "DECFLOAT(16)", true);
        assert_eq!(
            assign(&d16, dec("12345678901234567")),
            Ok(dec("1.234567890123457E+16"))
        );
        assert_eq!(assign(&d16, dec("1E+400")), Err(OUT_OF_RANGE));
        let d34 = col("DF", "", "DECFLOAT(34)", true);
        assert_eq!(assign(&d34, dec("1E+400")), Ok(dec("1E+400")));
    }
}
