// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values to and from a record's fields.

use db::record::datetime::{Date, Time};
use dds::{DataType, FieldDef};
use qdecimal::{Rounding, Shape};

use super::{Timestamp, Value, conversion, decfloat_error};
use crate::error::{Result, SqlError};

/// A LOB's bytes as its column's value: a `BLOB` binary, a `CLOB`
/// CP037 text, a `DBCLOB` UTF-16BE text.
pub fn lob_value(f: &FieldDef, bytes: &[u8]) -> Value {
    match f.data_type {
        DataType::Clob => Value::Str(ccsid::from_ebcdic(bytes)),
        DataType::Dbclob => {
            let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
            Value::Str(String::from_utf16_lossy(&units))
        }
        _ => Value::Bin(bytes.to_vec()),
    }
}

fn too_long(f: &FieldDef, n: usize) -> SqlError {
    SqlError::Overflow { what: format!("a value of length {n} is longer than field {} ({})", f.name, f.length) }
}

impl Value {
    /// From a field's stored value. A LOB field's value is its
    /// reference; [`lob_value`] makes the value of its bytes.
    pub fn from_field(f: &FieldDef, v: &db::record::Value) -> Result<Self> {
        Ok(match v {
            db::record::Value::Null => Self::Null,
            db::record::Value::Num(d) => Self::Num(d.clone()),
            db::record::Value::DecFloat(d) => Self::DecFloat(*d),
            db::record::Value::Char(b) => match f.data_type {
                DataType::Bytes => Self::Bin(b.clone()),
                DataType::Boolean => Self::Bool(b.first() == Some(&0xF1)),
                t if t.is_lob() => Self::Bin(b.clone()),
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
            // A float keeps every digit it is given; the encoding rounds
            // to the nearest float.
            DataType::Float => match self {
                Self::DecFloat(d) if !d.is_finite() => {
                    Err(conversion(format!("DECFLOAT {d} into the float field {}", f.name)))
                }
                Self::DecFloat(d) => super::float_decimal(d.to_f64())
                    .map(db::record::Value::Num)
                    .ok_or_else(|| conversion(format!("{d} is out of range for field {}", f.name))),
                _ => Ok(db::record::Value::Num(self.as_decimal()?)),
            },
            DataType::DecFloat => Ok(db::record::Value::DecFloat(
                self.as_decfloat(db::record::decfloat_format(f))?
                    .to_format(db::record::decfloat_format(f))
                    .map_err(decfloat_error)?,
            )),
            // An SQL integer: its bytes' range, the fraction cut off.
            DataType::Binary if f.int_range => {
                let d = self.as_decimal()?;
                let bits = f.data_len() * 8;
                let whole = d.rescale(0, Rounding::Truncate).ok().and_then(|w| w.to_i128());
                match whole {
                    Some(n) if n >= -(1i128 << (bits - 1)) && n < (1i128 << (bits - 1)) => {
                        Ok(db::record::Value::Num(qdecimal::Decimal::new(n, 0).map_err(conversion)?))
                    }
                    _ => Err(SqlError::Overflow { what: format!("{d} does not fit field {}", f.name) }),
                }
            }
            DataType::Zoned | DataType::Packed | DataType::Binary => {
                let d = self.as_decimal()?;
                let shape = Shape::new(u8::try_from(f.length).unwrap_or(63), f.decimals).map_err(conversion)?;
                let fitted = d
                    .fit(shape, Rounding::Truncate)
                    .map_err(|_| SqlError::Overflow { what: format!("{d} does not fit field {}", f.name) })?;
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
            DataType::Date => text(&self.as_date()?.render(f.date_fmt.unwrap_or_default(), f.date_sep)),
            DataType::Time => text(&self.as_time()?.stored(f.time_fmt.unwrap_or_default(), f.time_sep)),
            DataType::Timestamp => text(
                &self.as_timestamp()?.with_precision(DataType::timestamp_precision(f.length)).render_field(),
            ),
            _ => {
                let s = match self {
                    // A binary string into a bit-data field: its bytes.
                    Self::Bin(b) => ccsid::from_ebcdic(b),
                    v => v.as_text(),
                };
                let max = usize::try_from(f.length).expect("fits");
                if s.chars().count() > max && s.trim_end().chars().count() > max {
                    return Err(SqlError::Overflow { what: format!("'{s}' is longer than field {} ({max})", f.name) });
                }
                text(&s)
            }
        }
    }
}
