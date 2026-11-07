// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CAST` and the casting matrix: which source family may become which
//! type, and IBM's code when it may not or the value does not fit
//! (`SQL0461`, `SQL0420`, `SQL0802`, `SQL0180`, `SQL0181`, `SQL0433`,
//! `SQL0189`, `SQL0332`). The primary settled each rule; its outcomes
//! are `tools/conformance/sql/cases/types/CAST*.sql` and `DECFLT*.sql`.

pub mod arith;
pub mod number;
pub mod text;

use qdecimal::{DecFloat, DfFormat, DfRounding, Rounding, Shape};

use crate::ast::{DataType, TypeSpec};
use crate::error::{Result, SqlError};
use crate::value::{Timestamp, Value};

pub use text::{char_text, decfloat_char, float_char};

/// An error with IBM's code.
pub(crate) fn coded(code: i32, state: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Coded {
        code,
        state,
        what: what.into(),
    }
}

/// `SQL0420`: a character string that is not a number.
pub(crate) fn bad_chars(text: &str) -> SqlError {
    coded(
        -420,
        "22018",
        format!("'{text}' is not a valid number for the cast"),
    )
}

/// `SQL0420` for text that is no BOOLEAN: an error even in a select
/// list, where the other `SQL0420`s are a warning.
pub(crate) fn bad_boolean(text: &str) -> SqlError {
    coded(
        -420,
        "22018",
        format!("{BOOLEAN_TEXT}'{text}' is not TRUE or FALSE"),
    )
}

/// The start of a [`bad_boolean`] text.
const BOOLEAN_TEXT: &str = "BOOLEAN: ";

/// `SQL0802`: a number past its target.
pub(crate) fn out_of_range(what: impl std::fmt::Display) -> SqlError {
    coded(
        -802,
        "22003",
        format!("{what} is out of range for the result"),
    )
}

/// `SQL0433`: a value too long for the string it is cast to.
fn truncated(v: &Value, n: u32) -> SqlError {
    coded(
        -433,
        "22001",
        format!("{} does not fit in {n} characters", char_text(v)),
    )
}

/// `SQL0461`: a cast IBM does not allow.
pub(crate) fn not_castable(v: &Value, t: &DataType) -> SqlError {
    coded(
        -461,
        "42846",
        format!("a {} value cannot be cast to {}", family(v), type_name(t)),
    )
}

/// `SQL0189`: a CCSID the type cannot have.
fn bad_ccsid(ccsid: u16) -> SqlError {
    coded(
        -189,
        "22522",
        format!("CCSID {ccsid} is not valid for this data type"),
    )
}

/// `SQL0180` for text that is not a date, time or timestamp; `SQL0181`
/// for one in the right form whose values are out of range.
pub(crate) fn bad_datetime(text: &str, what: &str) -> SqlError {
    let groups = text
        .split(['-', '.', ':', '/', ' '])
        .filter(|g| !g.is_empty())
        .count();
    let shaped = text
        .trim()
        .bytes()
        .all(|b| b.is_ascii_digit() || b"-.:/ ".contains(&b))
        && groups >= 3;
    if shaped {
        coded(
            -181,
            "22007",
            format!("'{text}' is not a valid {what}: a value is out of range"),
        )
    } else {
        coded(-180, "22007", format!("'{text}' is not in a {what} form"))
    }
}

/// The family of a value, for messages.
fn family(v: &Value) -> &'static str {
    match v {
        Value::Num(_) => "numeric",
        Value::Double(_) | Value::Real(_) => "floating-point",
        Value::DecFloat(_) => "DECFLOAT",
        Value::Str(_) => "character",
        Value::Bin(_) => "binary",
        Value::Bool(_) => "BOOLEAN",
        Value::Date(_) => "DATE",
        Value::Time(_) => "TIME",
        Value::Timestamp(_) => "TIMESTAMP",
        Value::Array(_) => "array",
        Value::Null => "null",
    }
}

/// A type's name, for messages.
#[must_use]
pub fn type_name(t: &DataType) -> &'static str {
    match t {
        DataType::Char(_) => "CHAR",
        DataType::Varchar(_) => "VARCHAR",
        DataType::Graphic(_) => "GRAPHIC",
        DataType::Vargraphic(_) => "VARGRAPHIC",
        DataType::Decimal(..) => "DECIMAL",
        DataType::Numeric(..) => "NUMERIC",
        DataType::SmallInt => "SMALLINT",
        DataType::Integer => "INTEGER",
        DataType::BigInt => "BIGINT",
        DataType::Real => "REAL",
        DataType::Double => "DOUBLE",
        DataType::Date => "DATE",
        DataType::Time => "TIME",
        DataType::Timestamp(_) => "TIMESTAMP",
        DataType::DecFloat(_) => "DECFLOAT",
        DataType::Binary(_) => "BINARY",
        DataType::Varbinary(_) => "VARBINARY",
        DataType::Boolean => "BOOLEAN",
        DataType::Blob(_) => "BLOB",
        DataType::Clob(_) => "CLOB",
        DataType::Dbclob(_) => "DBCLOB",
    }
}

/// The CCSIDs a character type may name: the EBCDIC single-byte and
/// mixed ones, UTF-8 and 65535; a graphic type: UTF-16 (1200, 13488)
/// and the EBCDIC double-byte ones.
pub fn check_ccsid(t: &TypeSpec) -> Result<()> {
    let Some(c) = t.ccsid else { return Ok(()) };
    let graphic = matches!(
        t.data_type,
        DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_)
    );
    let character = matches!(
        t.data_type,
        DataType::Char(_) | DataType::Varchar(_) | DataType::Clob(_)
    );
    let codec = ccsid::conv::Codec::of(c).ok().flatten();
    let ok = if graphic {
        matches!(c, 1200 | 13488) || c == 300
    } else if character {
        c == 65535 || c == 1208 || codec.is_some_and(|k| k.is_ebcdic() && c != 300)
    } else {
        false
    };
    if ok { Ok(()) } else { Err(bad_ccsid(c)) }
}

fn format_of(n: u8) -> DfFormat {
    if n == 16 {
        DfFormat::D16
    } else {
        DfFormat::D34
    }
}

/// A `DECFLOAT` failure: `SQL0802` for overflow, underflow and division
/// by zero, `SQ20365` for a signaling NaN.
#[must_use]
pub fn decfloat_error(e: qdecimal::DecimalError) -> SqlError {
    use qdecimal::DecimalError as E;
    match e {
        E::DivideByZero => coded(-802, "22012", "division by zero"),
        E::Invalid { what } => coded(-20365, "22502", format!("DECFLOAT: {what}")),
        E::Parse { text } => bad_chars(&text),
        e => coded(-802, "22003", e.to_string()),
    }
}

/// The value as a `DECFLOAT` of `format`: a float's exact value, text
/// parsed (special values too), rounded in `mode`.
pub fn to_decfloat(v: &Value, format: DfFormat, mode: DfRounding) -> Result<DecFloat> {
    match v {
        Value::DecFloat(d) => d.to_format_in(format, mode).map_err(decfloat_error),
        Value::Num(d) => DecFloat::from_decimal_in(d, format, mode).map_err(decfloat_error),
        Value::Double(x) => DecFloat::from_f64_exact(*x, format, mode).map_err(decfloat_error),
        Value::Real(x) => DecFloat::from_f32_exact(*x, format, mode).map_err(decfloat_error),
        Value::Str(s) => DecFloat::parse_in(s.trim_matches(' '), format, mode)
            .map_err(|e| {
                if matches!(e, qdecimal::DecimalError::Parse { .. }) {
                    bad_chars(s)
                } else {
                    decfloat_error(e)
                }
            })
            .and_then(|d| {
                if d.is_finite() || !s.contains(' ') {
                    Ok(d)
                } else {
                    Err(bad_chars(s))
                }
            }),
        Value::Bool(b) => Ok(DecFloat::from_i64(i64::from(*b), format)),
        v => Err(not_castable(
            v,
            &DataType::DecFloat(if format == DfFormat::D16 { 16 } else { 34 }),
        )),
    }
}

/// The value as a decimal of `p, s`, the fraction cut off.
fn to_decimal(v: &Value, p: u8, s: u8) -> Result<Value> {
    let d = match v {
        Value::Str(t) => number::parse(t)?.decimal(s, t)?,
        Value::Bool(b) => qdecimal::Decimal::new(i128::from(*b), 0).expect("fits"),
        Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_) => {
            v.as_decimal().map_err(|_| out_of_range(char_text(v)))?
        }
        v => return Err(not_castable(v, &DataType::Decimal(p, s))),
    };
    let shape = Shape::new(p, s).map_err(|e| coded(-604, "42611", e.to_string()))?;
    d.fit(shape, Rounding::Truncate)
        .map(Value::Num)
        .map_err(|_| out_of_range(&d))
}

/// An integer of `bits`.
fn to_integer(v: &Value, t: &DataType, bits: u32) -> Result<Value> {
    let d = match v {
        Value::Str(s) => number::parse(s)?.decimal(0, s)?,
        Value::Bool(b) => qdecimal::Decimal::new(i128::from(*b), 0).expect("fits"),
        Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_) => {
            v.as_decimal().map_err(|_| out_of_range(char_text(v)))?
        }
        v => return Err(not_castable(v, t)),
    };
    let whole = d
        .rescale(0, Rounding::Truncate)
        .ok()
        .and_then(|w| w.to_i128());
    match whole {
        Some(n) if n >= -(1i128 << (bits - 1)) && n < (1i128 << (bits - 1)) => {
            Ok(Value::int(i64::try_from(n).expect("in range")))
        }
        _ => Err(out_of_range(&d)),
    }
}

/// A double.
fn to_double(v: &Value, t: &DataType) -> Result<f64> {
    match v {
        Value::Str(s) => {
            number::parse(s)?;
            let x: f64 = s.trim_matches(' ').parse().map_err(|_| bad_chars(s))?;
            if x.is_infinite() {
                return Err(out_of_range(s));
            }
            Ok(x)
        }
        Value::Bool(b) => Ok(f64::from(u8::from(*b))),
        Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_) => {
            let x = v.as_f64()?;
            if x.is_infinite() && !matches!(v, Value::Double(_) | Value::Real(_)) && !is_special(v)
            {
                return Err(out_of_range(char_text(v)));
            }
            Ok(x)
        }
        v => Err(not_castable(v, t)),
    }
}

fn is_special(v: &Value) -> bool {
    matches!(v, Value::DecFloat(d) if !d.is_finite())
}

/// Text into a fixed or varying string of `n` characters: a value that
/// is not a string must fit (`SQL0433`); a string is cut off.
fn to_string(v: &Value, n: u32, fixed: bool) -> Result<Value> {
    let max = usize::try_from(n).expect("fits");
    let is_text = matches!(v, Value::Str(_) | Value::Bin(_) | Value::Bool(_));
    let s = match v {
        Value::Bin(b) => ccsid::from_ebcdic(b),
        v => char_text(v),
    };
    let len = s.chars().count();
    let mut out: String = if len > max {
        if !is_text {
            return Err(truncated(v, n));
        }
        s.chars().take(max).collect()
    } else {
        s
    };
    if fixed {
        let have = out.chars().count();
        out.extend(std::iter::repeat_n(' ', max - have.min(max)));
    }
    Ok(Value::Str(out))
}

/// Bytes of `n`, padded with zeros when `fixed`.
fn to_binary(v: &Value, t: &DataType, n: u32, fixed: bool) -> Result<Value> {
    let mut b = match v {
        Value::Bin(b) => b.clone(),
        Value::Str(_) => v.as_bytes()?,
        v => return Err(not_castable(v, t)),
    };
    let n = usize::try_from(n).expect("fits");
    b.truncate(n);
    if fixed {
        b.resize(n, 0);
    }
    Ok(Value::Bin(b))
}

/// `CAST(v AS t)`, `DECFLOAT` results rounded in `mode`.
pub fn cast(v: &Value, t: &TypeSpec, mode: DfRounding) -> Result<Value> {
    check_ccsid(t)?;
    if v.is_null() {
        return Ok(Value::Null);
    }
    let dt = &t.data_type;
    if matches!(
        dt,
        DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_)
    ) && matches!(v, Value::Bin(_))
    {
        return Err(coded(
            -332,
            "57017",
            "binary data has no CCSID a graphic string can be converted from",
        ));
    }
    Ok(match dt {
        // A BOOLEAN is no number on IBM i 7.5.
        _ if matches!(v, Value::Bool(_)) && crate::exec::restype::unify::is_number(dt) => {
            return Err(not_castable(v, dt));
        }
        DataType::DecFloat(n) => Value::DecFloat(to_decfloat(v, format_of(*n), mode)?),
        DataType::Decimal(p, s) | DataType::Numeric(p, s) => to_decimal(v, *p, *s)?,
        DataType::SmallInt => to_integer(v, dt, 16)?,
        DataType::Integer => to_integer(v, dt, 32)?,
        DataType::BigInt => to_integer(v, dt, 64)?,
        DataType::Double => Value::Double(to_double(v, dt)?),
        DataType::Real => {
            let x = to_double(v, dt)?;
            #[allow(clippy::cast_possible_truncation)] // the single it rounds to
            let r = x as f32;
            if r.is_infinite() && x.is_finite() {
                return Err(out_of_range(char_text(v)));
            }
            Value::Real(r)
        }
        DataType::Char(n) | DataType::Graphic(n) => to_string(v, *n, true)?,
        DataType::Varchar(n)
        | DataType::Vargraphic(n)
        | DataType::Clob(n)
        | DataType::Dbclob(n) => to_string(v, *n, false)?,
        DataType::Binary(n) => to_binary(v, dt, *n, true)?,
        DataType::Varbinary(n) | DataType::Blob(n) => to_binary(v, dt, *n, false)?,
        // Text only: a number is no BOOLEAN on IBM i 7.5.
        DataType::Boolean => match v {
            Value::Bool(b) => Value::Bool(*b),
            Value::Str(s) => {
                Value::Bool(crate::value::parse_bool(s).ok_or_else(|| bad_boolean(s))?)
            }
            v => return Err(not_castable(v, dt)),
        },
        DataType::Date => match v {
            Value::Date(d) => Value::Date(*d),
            Value::Timestamp(ts) => Value::Date(ts.date),
            Value::Str(s) => Value::Date(cast_date(s).ok_or_else(|| bad_datetime(s, "date"))?),
            v => return Err(not_castable(v, dt)),
        },
        DataType::Time => match v {
            Value::Time(x) => Value::Time(*x),
            Value::Timestamp(ts) => Value::Time(ts.time),
            Value::Str(s) => Value::Time(
                crate::value::parse_time(s.trim()).ok_or_else(|| bad_datetime(s, "time"))?,
            ),
            v => return Err(not_castable(v, dt)),
        },
        DataType::Timestamp(p) => match v {
            Value::Timestamp(ts) => Value::Timestamp(ts.with_precision(*p)),
            Value::Date(d) => Value::Timestamp(Timestamp::midnight(*d).with_precision(*p)),
            Value::Str(s) => Value::Timestamp(
                Timestamp::parse(s.trim())
                    .ok_or_else(|| bad_datetime(s, "timestamp"))?
                    .with_precision(*p),
            ),
            v => return Err(not_castable(v, dt)),
        },
    })
}

#[cfg(test)]
mod tests;

/// A result column's value: when the statement maps them, a data
/// mapping error (`SQL0802`, `SQL0420`, `SQL0180`, `SQL0181`,
/// `SQ20365`) makes the value null and the statement's warning, as the
/// primary returns a select list's errors.
pub fn mapped(ctx: &mut crate::exec::SqlContext, r: Result<Value>) -> Result<Value> {
    let Err(e) = r else { return r };
    if !ctx.map_errors
        || matches!(&e, SqlError::Coded { what, .. } if what.starts_with(BOOLEAN_TEXT))
    {
        return Err(e);
    }
    let warning = match (e.sqlcode(), e.sqlstate()) {
        // A result too long for its string type fails at fetch.
        (-802, "22001") => return Err(e),
        (-802, "22012") => (802, "01564"),
        (-802, _) => (802, "01519"),
        (-420, _) => (420, "01565"),
        (-180 | -181, _) => (180, "01534"),
        (-20365, _) => (20365, "01565"),
        _ => return Err(e),
    };
    ctx.mapping_warning.get_or_insert(warning);
    Ok(Value::Null)
}

impl crate::exec::SqlContext {
    /// `SET CURRENT DECFLOAT ROUNDING MODE`.
    pub(crate) fn set_decfloat_rounding(&mut self, mode: DfRounding) -> crate::exec::Outcome {
        self.decfloat_rounding = mode;
        crate::exec::Outcome::Done
    }
}

/// A date from text as a `CAST` reads it: ISO, USA, EUR or JIS.
fn cast_date(s: &str) -> Option<db::record::datetime::Date> {
    use dds::DateFmt;
    let t = s.trim();
    // A date has separators: `20261105` is not one.
    if !t.contains(['-', '/', '.']) {
        return None;
    }
    [DateFmt::Iso, DateFmt::Usa, DateFmt::Eur, DateFmt::Jis]
        .iter()
        .find_map(|f| db::record::datetime::Date::parse(t, *f))
}

/// A column's type options: its CCSID (`SQL0189`), and `FOR BIT DATA`
/// only on `CHAR` and `VARCHAR` (`SQL0683`).
pub fn check_column(t: &TypeSpec) -> Result<()> {
    check_ccsid(t)?;
    if t.bit_data && !matches!(t.data_type, DataType::Char(_) | DataType::Varchar(_)) {
        return Err(coded(
            -683,
            "42842",
            format!("FOR BIT DATA is not valid for {}", type_name(&t.data_type)),
        ));
    }
    Ok(())
}
