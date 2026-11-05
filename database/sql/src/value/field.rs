// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values to and from a record's fields.

use db::record::datetime::{Date, Time};
use db::record::utf16_text;
use dds::{DataType, FieldDef};
use qdecimal::{Rounding, Shape};

use super::{Timestamp, Value, conversion, decfloat_error};
use crate::exec::cast::{coded, number};
use crate::error::{Result, SqlError};

/// A LOB's bytes as its column's value: a `BLOB` binary, a `CLOB`
/// CP037 text, a `DBCLOB` UTF-16BE text.
pub fn lob_value(f: &FieldDef, bytes: &[u8]) -> Value {
    match f.data_type {
        DataType::Clob => Value::Str(ccsid::from_ebcdic(bytes)),
        DataType::Dbclob => Value::Str(utf16_text(bytes)),
        _ => Value::Bin(bytes.to_vec()),
    }
}

/// `SQL0404`: a string longer than its column.
fn too_long(f: &FieldDef, n: usize) -> SqlError {
    coded(-404, "22001", format!("a value of length {n} is longer than column {} ({})", f.name, f.length))
}

/// `SQL0406`: a number past its column.
fn misfit(f: &FieldDef, v: impl std::fmt::Display) -> SqlError {
    coded(-406, "22003", format!("{v} does not fit column {}", f.name))
}

/// `SQL0408`: a value of a type the column cannot take.
fn incompatible(f: &FieldDef, v: &Value) -> SqlError {
    coded(-408, "42821", format!("a {} value cannot be assigned to column {}", v.type_name(), f.name))
}

impl Value {
    /// The value as a number for a numeric column: text read as a
    /// `CAST` reads it (`SQL0420`), a date or binary string refused.
    fn assign_decimal(&self, f: &FieldDef) -> Result<qdecimal::Decimal> {
        match self {
            Self::Str(s) => number::parse(s)?.decimal(63, s).map_err(|_| misfit(f, s)),
            Self::Bool(b) => Ok(qdecimal::Decimal::new(i128::from(*b), 0).expect("fits")),
            Self::Num(_) | Self::Double(_) | Self::Real(_) | Self::DecFloat(_) => {
                self.as_decimal().map_err(|_| misfit(f, self.as_text()))
            }
            v => Err(incompatible(f, v)),
        }
    }

    /// Why the value is no date, time or timestamp for column `f`.
    fn datetime_misfit(&self, f: &FieldDef, what: &str) -> SqlError {
        match self {
            Self::Str(s) => crate::exec::cast::bad_datetime(s, what),
            v => incompatible(f, v),
        }
    }

    /// The value as a double for a float column.
    fn assign_f64(&self, f: &FieldDef) -> Result<f64> {
        match self {
            Self::Str(s) => {
                number::parse(s)?;
                s.trim_matches(' ').parse().map_err(|_| misfit(f, s))
            }
            Self::Bool(_) | Self::Num(_) | Self::Double(_) | Self::Real(_) | Self::DecFloat(_) => self.as_f64(),
            v => Err(incompatible(f, v)),
        }
    }
}

impl Value {
    /// From a field's stored value. A LOB field's value is its
    /// reference; [`lob_value`] makes the value of its bytes.
    pub fn from_field(f: &FieldDef, v: &db::record::Value) -> Result<Self> {
        Ok(match v {
            db::record::Value::Null => Self::Null,
            db::record::Value::Num(d) => Self::Num(d.clone()),
            #[allow(clippy::cast_possible_truncation)] // a single, widened when read
            db::record::Value::Float(x) if f.precision == dds::FloatPrecision::Single => Self::Real(*x as f32),
            db::record::Value::Float(x) => Self::Double(*x),
            db::record::Value::DecFloat(d) => Self::DecFloat(*d),
            db::record::Value::Char(b) => match f.data_type {
                DataType::Bytes => Self::Bin(b.clone()),
                DataType::Boolean => Self::Bool(b.first() == Some(&0xF1)),
                t if t.is_lob() => Self::Bin(b.clone()),
                _ if db::record::is_utf16(f) => Self::Str(utf16_text(b)),
                _ => {
                    let s = ccsid::from_ebcdic(b);
                    match f.data_type {
                        DataType::Date => Self::Date(
                            Date::parse(&s, f.date_fmt.unwrap_or_default())
                                .ok_or_else(|| conversion(format!("field {} holds '{s}', not a date", f.name)))?,
                        ),
                        DataType::Time => Self::Time(
                            Time::parse(&s, f.time_fmt.unwrap_or_default())
                                .ok_or_else(|| conversion(format!("field {} holds '{s}', not a time", f.name)))?,
                        ),
                        DataType::Timestamp => Self::Timestamp(
                            Timestamp::parse(&s)
                                .ok_or_else(|| conversion(format!("field {} holds '{s}', not a timestamp", f.name)))?
                                .with_precision(DataType::timestamp_precision(f.length)),
                        ),
                        _ => Self::Str(s),
                    }
                }
            },
        })
    }

    /// The bytes a LOB field keeps for this value, checked against the
    /// field's largest length.
    pub fn lob_bytes(&self, f: &FieldDef) -> Result<Vec<u8>> {
        let max = usize::try_from(f.length).expect("fits");
        let bytes = match (f.data_type, self) {
            (DataType::Dbclob, v) => {
                let s = v.as_text();
                let units: Vec<u16> = s.encode_utf16().collect();
                if units.len() > max {
                    return Err(too_long(f, units.len()));
                }
                return Ok(units.iter().flat_map(|u| u.to_be_bytes()).collect());
            }
            (DataType::Clob, Self::Bin(b)) => b.clone(),
            (DataType::Clob, v) => Self::Str(v.as_text()).as_bytes()?,
            (_, v) => v.as_bytes()?,
        };
        if bytes.len() > max {
            return Err(too_long(f, bytes.len()));
        }
        Ok(bytes)
    }

    /// Into a field's value: text to CP037, dates and times in the
    /// field's format, decimals fitted to the field (truncated), floats
    /// and `DECFLOAT`s as they are. A LOB field takes its reference
    /// apart ([`Self::lob_bytes`]).
    pub fn to_field(&self, f: &FieldDef) -> Result<db::record::Value> {
        if self.is_null() {
            return Ok(db::record::Value::Null);
        }
        let text = |s: &str| -> Result<db::record::Value> {
            ccsid::to_ebcdic(s)
                .map(db::record::Value::Char)
                .map_err(|_| conversion(format!("'{s}' has characters CP037 cannot hold")))
        };
        match f.data_type {
            // A float: its IEEE value; a single's range is checked
            // when it is encoded.
            DataType::Float => {
                let x = self.assign_f64(f)?;
                #[allow(clippy::cast_possible_truncation)] // the single it rounds to
                if f.precision == dds::FloatPrecision::Single && x.is_finite() && (x as f32).is_infinite() {
                    return Err(misfit(f, self.as_text()));
                }
                Ok(db::record::Value::Float(x))
            }
            DataType::DecFloat => Ok(db::record::Value::DecFloat(
                self.as_decfloat(db::record::decfloat_format(f))?
                    .to_format(db::record::decfloat_format(f))
                    .map_err(decfloat_error)?,
            )),
            // An SQL integer: its bytes' range, the fraction cut off.
            DataType::Binary if f.int_range => {
                let d = self.assign_decimal(f)?;
                let bits = f.data_len() * 8;
                let whole = d.rescale(0, Rounding::Truncate).ok().and_then(|w| w.to_i128());
                match whole {
                    Some(n) if n >= -(1i128 << (bits - 1)) && n < (1i128 << (bits - 1)) => {
                        Ok(db::record::Value::Num(qdecimal::Decimal::new(n, 0).map_err(conversion)?))
                    }
                    _ => Err(misfit(f, &d)),
                }
            }
            DataType::Zoned | DataType::Packed | DataType::Binary => {
                let d = self.assign_decimal(f)?;
                let shape = Shape::new(u8::try_from(f.length).unwrap_or(63), f.decimals).map_err(conversion)?;
                let fitted = d
                    .fit(shape, Rounding::Truncate)
                    .map_err(|_| misfit(f, &d))?;
                Ok(db::record::Value::Num(fitted))
            }
            DataType::Bytes => {
                let b = self.as_bytes()?;
                if b.len() > usize::try_from(f.length).expect("fits") {
                    return Err(too_long(f, b.len()));
                }
                Ok(db::record::Value::Char(b))
            }
            DataType::Boolean => Ok(db::record::Value::Char(vec![if self.as_bool()? { 0xF1 } else { 0xF0 }])),
            DataType::Blob | DataType::Clob | DataType::Dbclob => Err(SqlError::Database {
                what: format!("LOB field {} is written through its reference", f.name),
                msgid: None,
            }),
            DataType::Date => text(&self.as_date().map_err(|_| self.datetime_misfit(f, "date"))?.render(f.date_fmt.unwrap_or_default(), f.date_sep)),
            DataType::Time => text(&self.as_time().map_err(|_| self.datetime_misfit(f, "time"))?.stored(f.time_fmt.unwrap_or_default(), f.time_sep)),
            DataType::Timestamp => text(
                &self.as_timestamp().map_err(|_| self.datetime_misfit(f, "timestamp"))?.with_precision(DataType::timestamp_precision(f.length)).render_field(),
            ),
            _ => {
                let s = match self {
                    // A binary string into a bit-data field: its bytes.
                    Self::Bin(b) => ccsid::from_ebcdic(b),
                    v => v.as_text(),
                };
                let max = usize::try_from(f.length).expect("fits");
                // A UTF-16 graphic field holds any character, counted
                // in UTF-16 units.
                if db::record::is_utf16(f) {
                    let units: Vec<u16> = s.encode_utf16().collect();
                    let trimmed = s.trim_end().encode_utf16().count();
                    if units.len() > max && trimmed > max {
                        return Err(too_long(f, s.chars().count()));
                    }
                    return Ok(db::record::Value::Char(units.iter().take(max).flat_map(|u| u.to_be_bytes()).collect()));
                }
                if s.chars().count() > max && s.trim_end().chars().count() > max {
                    return Err(too_long(f, s.chars().count()));
                }
                text(&s)
            }
        }
    }
}
