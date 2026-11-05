// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL values: what expressions produce and rows hold. Text is UTF-8
//! here and CP037 in the record; the field's attributes decide the
//! conversion both ways.

mod field;
mod timestamp;

use std::cmp::Ordering;
use std::fmt;

use db::record::datetime::{Date, Time};
use dds::{DateFmt, TimeFmt};
use qdecimal::{DecFloat, Decimal, DecimalError, DfFormat, DfRounding, Rounding};

use crate::error::{Result, SqlError};

pub use field::lob_value;
pub use timestamp::{PICOS, Timestamp};

/// A value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// The null value.
    Null,
    /// A number: integers and decimals.
    Num(Decimal),
    /// A `DOUBLE` (`FLOAT`), IEEE binary.
    Double(f64),
    /// A `REAL`, IEEE single.
    Real(f32),
    /// A `DECFLOAT`.
    DecFloat(DecFloat),
    /// A character string (`CLOB` and `DBCLOB` too).
    Str(String),
    /// A binary string: `BINARY`, `VARBINARY`, `BLOB`.
    Bin(Vec<u8>),
    /// A `BOOLEAN`.
    Bool(bool),
    /// A date.
    Date(Date),
    /// A time.
    Time(Time),
    /// A timestamp.
    Timestamp(Timestamp),
    /// An array (`CREATE TYPE ... AS ARRAY`): its elements, in SQL PL
    /// only.
    Array(Vec<Value>),
}

// Floats compare by value.
impl Eq for Value {}

/// Dates in order.
#[must_use]
pub fn cmp_date(a: &Date, b: &Date) -> Ordering {
    (a.year, a.month, a.day).cmp(&(b.year, b.month, b.day))
}

/// Times in order.
#[must_use]
pub fn cmp_time(a: &Time, b: &Time) -> Ordering {
    (a.hour, a.minute, a.second).cmp(&(b.hour, b.minute, b.second))
}

fn conversion(what: impl fmt::Display) -> SqlError {
    SqlError::Conversion { what: what.to_string() }
}

/// A `DECFLOAT` failure as SQL reports it.
#[must_use]
pub fn decfloat_error(e: DecimalError) -> SqlError {
    crate::exec::cast::decfloat_error(e)
}

/// Text compared with a number, as the number's type; text that is not
/// a number is `SQL0302`, as the primary reports it.
fn number_of(s: &str, num: &Value) -> Result<Value> {
    crate::exec::cast::arith::string_as(s, num).map_err(|_| SqlError::Coded {
        code: -302,
        state: "22023",
        what: format!("'{s}' is not a number to compare with {num}"),
    })
}

/// A date from text in the ISO, USA, EUR or two-digit forms.
#[must_use]
pub fn parse_date(text: &str) -> Option<Date> {
    [DateFmt::Iso, DateFmt::Usa, DateFmt::Eur, DateFmt::Mdy, DateFmt::Jul].iter().find_map(|f| Date::parse(text, *f))
}

/// A finite double as a decimal, its shortest exact text; `None` past
/// 63 digits.
#[must_use]
pub fn float_decimal(f: f64) -> Option<Decimal> {
    db::record::float_decimal(f)
}

/// A finite single as a decimal, its shortest text as a single.
fn real_decimal(f: f32) -> Option<Decimal> {
    f.is_finite().then(|| Decimal::parse(&format!("{f}")).ok()).flatten()
}

/// A float from its text: digits, E notation, `Infinity`, `-Infinity`
/// or `NaN`.
#[must_use]
pub fn parse_float(text: &str) -> Option<f64> {
    match text.trim() {
        "Infinity" | "+Infinity" | "INF" => Some(f64::INFINITY),
        "-Infinity" | "-INF" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        t => t.parse().ok(),
    }
}

/// A single's shortest text that reads back the same, E notation.
fn real_text(f: f32) -> String {
    if f.is_finite() { format!("{f:E}") } else { db::record::float_text(f64::from(f)) }
}

/// A float too large for a decimal.
fn float_range(x: f64) -> SqlError {
    SqlError::Overflow { what: format!("{} is out of range for a decimal", db::record::float_text(x)) }
}

/// A time from text in any format.
#[must_use]
pub fn parse_time(text: &str) -> Option<Time> {
    Time::parse(text, TimeFmt::Hms).or_else(|| Time::parse(text, TimeFmt::Usa))
}

/// The truth of a word, as `CAST(x AS BOOLEAN)` reads it.
#[must_use]
pub fn parse_bool(text: &str) -> Option<bool> {
    match text.trim().to_ascii_uppercase().as_str() {
        "TRUE" | "T" | "YES" | "Y" | "ON" | "1" => Some(true),
        "FALSE" | "F" | "NO" | "N" | "OFF" | "0" => Some(false),
        _ => None,
    }
}

/// Bytes as hexadecimal digits.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// Hexadecimal digits as bytes.
#[must_use]
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let t = text.trim();
    if !t.len().is_multiple_of(2) {
        return None;
    }
    (0..t.len()).step_by(2).map(|i| t.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

/// A decimal's format when it meets a `DECFLOAT`: 16 digits or fewer
/// stay `DECFLOAT(16)`.
fn format_of(d: &Decimal) -> DfFormat {
    if d.digits_needed() <= 16 { DfFormat::D16 } else { DfFormat::D34 }
}

impl Value {
    /// Whether null.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// An integer.
    #[must_use]
    pub fn int(n: i64) -> Self {
        Self::Num(Decimal::new(i128::from(n), 0).expect("fits"))
    }

    /// The value as a number; text is parsed.
    pub fn as_decimal(&self) -> Result<Decimal> {
        match self {
            Self::Num(d) => Ok(d.clone()),
            Self::Double(x) => float_decimal(*x).ok_or_else(|| float_range(*x)),
            Self::Real(x) => real_decimal(*x).ok_or_else(|| float_range(f64::from(*x))),
            Self::DecFloat(d) => d.to_decimal().map_err(|e| conversion(format!("DECFLOAT {d}: {e}"))),
            Self::Str(s) => Decimal::parse(s.trim())
                .or_else(|_| DecFloat::parse(s, DfFormat::D34).and_then(|d| d.to_decimal()))
                .map_err(|_| conversion(format!("'{s}' is not a number"))),
            v => Err(conversion(format!("{} is not a number", v.type_name()))),
        }
    }

    /// The value as a `DECFLOAT` of `format` or wider; text is parsed.
    pub fn as_decfloat(&self, format: DfFormat) -> Result<DecFloat> {
        match self {
            Self::DecFloat(d) => Ok(*d),
            Self::Num(d) => DecFloat::from_decimal(d, format).map_err(decfloat_error),
            // A float's exact binary value, as IBM i converts it.
            Self::Double(x) => DecFloat::from_f64_exact(*x, format, DfRounding::HalfEven).map_err(decfloat_error),
            Self::Real(x) => DecFloat::from_f32_exact(*x, format, DfRounding::HalfEven).map_err(decfloat_error),
            Self::Str(s) => DecFloat::parse(s, format).map_err(decfloat_error),
            v => Err(conversion(format!("{} is not a number", v.type_name()))),
        }
    }

    /// The value as a double: a decimal or `DECFLOAT` to the nearest,
    /// text parsed.
    pub fn as_f64(&self) -> Result<f64> {
        match self {
            Self::Double(x) => Ok(*x),
            Self::Real(x) => Ok(f64::from(*x)),
            Self::Num(d) => Ok(db::record::decimal_f64(d)),
            Self::DecFloat(d) => Ok(d.to_f64()),
            Self::Str(s) => parse_float(s).ok_or_else(|| conversion(format!("'{s}' is not a number"))),
            v => Err(conversion(format!("{} is not a number", v.type_name()))),
        }
    }

    /// Whether a `REAL` or `DOUBLE`.
    #[must_use]
    pub fn is_float(&self) -> bool {
        matches!(self, Self::Double(_) | Self::Real(_))
    }

    /// The value as an integer.
    pub fn as_i64(&self) -> Result<i64> {
        if self.is_float() {
            let x = self.as_f64()?.trunc();
            // i64::MAX as a double is 2^63, one past the range.
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            return if x >= -(i64::MAX as f64) - 1.0 && x < i64::MAX as f64 {
                Ok(x as i64)
            } else {
                Err(conversion(format!("{} is too large for an integer", self.as_text())))
            };
        }
        let d = self.as_decimal()?;
        let t = d.rescale(0, Rounding::Truncate).map_err(conversion)?;
        t.to_i128()
            .and_then(|n| i64::try_from(n).ok())
            .ok_or_else(|| conversion(format!("{d} is too large for an integer")))
    }

    /// The value as text.
    #[must_use]
    pub fn as_text(&self) -> String {
        self.to_string()
    }

    /// The value as bytes: a binary string's own, text in CP037.
    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Bin(b) => Ok(b.clone()),
            Self::Str(s) => {
                ccsid::to_ebcdic(s).map_err(|_| conversion(format!("'{s}' has characters CP037 cannot hold")))
            }
            v => Err(conversion(format!("{} is not a binary string", v.type_name()))),
        }
    }

    /// The value as a truth value.
    pub fn as_bool(&self) -> Result<bool> {
        match self {
            Self::Bool(b) => Ok(*b),
            Self::Num(d) => Ok(!d.is_zero()),
            Self::Double(_) | Self::Real(_) => Ok(self.as_f64()? != 0.0),
            Self::Str(s) => parse_bool(s).ok_or_else(|| conversion(format!("'{s}' is not a boolean"))),
            v => Err(conversion(format!("{} is not a boolean", v.type_name()))),
        }
    }

    /// The value as a date; text is parsed.
    pub fn as_date(&self) -> Result<Date> {
        match self {
            Self::Date(d) => Ok(*d),
            Self::Timestamp(t) => Ok(t.date),
            Self::Str(s) => parse_date(s).ok_or_else(|| conversion(format!("'{s}' is not a date"))),
            v => Err(conversion(format!("{} is not a date", v.type_name()))),
        }
    }

    /// The value as a time; text is parsed.
    pub fn as_time(&self) -> Result<Time> {
        match self {
            Self::Time(t) => Ok(*t),
            Self::Timestamp(t) => Ok(t.time),
            Self::Str(s) => parse_time(s).ok_or_else(|| conversion(format!("'{s}' is not a time"))),
            v => Err(conversion(format!("{} is not a time", v.type_name()))),
        }
    }

    /// The value as a timestamp; text is parsed, a date gets midnight.
    pub fn as_timestamp(&self) -> Result<Timestamp> {
        match self {
            Self::Timestamp(t) => Ok(*t),
            Self::Date(d) => Ok(Timestamp::midnight(*d)),
            Self::Str(s) => Timestamp::parse(s).ok_or_else(|| conversion(format!("'{s}' is not a timestamp"))),
            v => Err(conversion(format!("{} is not a timestamp", v.type_name()))),
        }
    }

    /// The SQL type family, for messages.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "NULL",
            Self::Num(_) => "numeric",
            Self::Double(_) | Self::Real(_) => "floating-point",
            Self::DecFloat(_) => "DECFLOAT",
            Self::Str(_) => "character",
            Self::Bin(_) => "binary",
            Self::Bool(_) => "boolean",
            Self::Date(_) => "date",
            Self::Time(_) => "time",
            Self::Timestamp(_) => "timestamp",
            Self::Array(_) => "array",
        }
    }

    /// Compare two values; `None` when either is null. Text compares in
    /// EBCDIC order with blank padding; text against a date, time or
    /// timestamp is parsed; numbers and `DECFLOAT`s by value with SQL's
    /// order of the special values; binary strings byte by byte, a
    /// shorter one first; a number against text is an error.
    pub fn compare(&self, other: &Self) -> Result<Option<Ordering>> {
        Ok(Some(match (self, other) {
            (Self::Null, _) | (_, Self::Null) => return Ok(None),
            (Self::Num(a), Self::Num(b)) => a.cmp(b),
            // A float beside a decimal: both as doubles; beside a
            // `DECFLOAT`: both as `DECFLOAT`.
            (Self::Double(_) | Self::Real(_), Self::Double(_) | Self::Real(_) | Self::Num(_))
            | (Self::Num(_), Self::Double(_) | Self::Real(_)) => db::record::fcmp(self.as_f64()?, other.as_f64()?),
            (Self::Double(_) | Self::Real(_), Self::DecFloat(b)) => self.as_decfloat(DfFormat::D34)?.sql_cmp(b),
            (Self::DecFloat(a), Self::Double(_) | Self::Real(_)) => a.sql_cmp(&other.as_decfloat(DfFormat::D34)?),
            (Self::DecFloat(a), Self::DecFloat(b)) => a.sql_cmp(b),
            (Self::DecFloat(a), Self::Num(b)) => a.sql_cmp(&DecFloat::from_decimal(b, DfFormat::D34).map_err(decfloat_error)?),
            (Self::Num(a), Self::DecFloat(b)) => DecFloat::from_decimal(a, DfFormat::D34).map_err(decfloat_error)?.sql_cmp(b),
            (Self::Str(a), Self::Str(b)) => ccsid::ebcdic_cmp_str(a.trim_end(), b.trim_end()),
            (Self::Bin(a), Self::Bin(b)) => a.cmp(b),
            // A number beside text: the text read as the number's type.
            (Self::Num(_) | Self::Double(_) | Self::Real(_) | Self::DecFloat(_), Self::Str(s)) => {
                return self.compare(&number_of(s, self)?);
            }
            (Self::Str(s), Self::Num(_) | Self::Double(_) | Self::Real(_) | Self::DecFloat(_)) => {
                return number_of(s, other)?.compare(other);
            }
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            // Predicates give 0 and 1.
            (Self::Bool(a), Self::Num(_) | Self::Double(_) | Self::Real(_)) => a.cmp(&other.as_bool()?),
            (Self::Num(_) | Self::Double(_) | Self::Real(_), Self::Bool(b)) => self.as_bool()?.cmp(b),
            (Self::Date(a), Self::Date(b)) => cmp_date(a, b),
            (Self::Time(a), Self::Time(b)) => cmp_time(a, b),
            (Self::Timestamp(a), Self::Timestamp(b)) => a.cmp(b),
            (Self::Date(a), Self::Str(_)) => cmp_date(a, &other.as_date()?),
            (Self::Str(_), Self::Date(b)) => cmp_date(&self.as_date()?, b),
            (Self::Time(a), Self::Str(_)) => cmp_time(a, &other.as_time()?),
            (Self::Str(_), Self::Time(b)) => cmp_time(&self.as_time()?, b),
            (Self::Timestamp(a), Self::Str(_)) => a.cmp(&other.as_timestamp()?),
            (Self::Str(_), Self::Timestamp(b)) => self.as_timestamp()?.cmp(b),
            (a, b) => {
                return Err(SqlError::TypeMismatch { what: format!("{} with {}", a.type_name(), b.type_name()) });
            }
        }))
    }

    /// A canonical text for grouping and `DISTINCT`: equal values give
    /// equal text.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Null => "\u{0}".into(),
            Self::Num(d) => decimal_key(d),
            Self::Double(x) => float_decimal(*x).map_or_else(|| db::record::float_text(*x), |d| decimal_key(&d)),
            Self::Real(x) => real_decimal(*x).map_or_else(|| real_text(*x), |d| decimal_key(&d)),
            Self::DecFloat(d) => d.to_decimal().map_or_else(|_| format!("#{d}"), |x| decimal_key(&x)),
            Self::Str(s) => format!("'{}", s.trim_end()),
            Self::Bin(b) => format!("x{}", hex(b)),
            Self::Timestamp(t) => t.key(),
            other => other.to_string(),
        }
    }

    /// The decimal format a number takes beside a `DECFLOAT`.
    #[must_use]
    pub fn decfloat_format(&self) -> DfFormat {
        match self {
            Self::DecFloat(d) => d.format(),
            Self::Num(d) => format_of(d),
            // A float beside a `DECFLOAT(16)` stays in 16 digits.
            Self::Double(_) | Self::Real(_) => DfFormat::D16,
            _ => DfFormat::D34,
        }
    }
}

fn decimal_key(d: &Decimal) -> String {
    let s = d.to_string();
    if s.contains('.') {
        let t = s.trim_end_matches('0').trim_end_matches('.');
        if t == "-0" || t.is_empty() { "0".into() } else { t.to_string() }
    } else {
        s
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => f.write_str("NULL"),
            Self::Num(d) => write!(f, "{d}"),
            Self::Double(x) => f.write_str(&db::record::float_display(*x)),
            Self::Real(x) => f.write_str(&db::record::single_display(*x)),
            Self::DecFloat(d) => write!(f, "{d}"),
            Self::Str(s) => f.write_str(s),
            Self::Bin(b) => f.write_str(&hex(b)),
            Self::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Self::Date(d) => f.write_str(&d.render(DateFmt::Iso, None)),
            Self::Time(t) => f.write_str(&t.render(TimeFmt::Iso, None)),
            Self::Timestamp(t) => write!(f, "{t}"),
            Self::Array(a) => {
                f.write_str("[")?;
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
        }
    }
}

#[cfg(test)]
mod tests;
