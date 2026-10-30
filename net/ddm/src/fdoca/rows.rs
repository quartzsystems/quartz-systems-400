// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values and rows as FD:OCA data. A `QRYDTA` row is an SQLCA group
//! (null for an ordinary row; `+100` or an error ends the data) then
//! the column group: a null indicator for the group, then each column,
//! a nullable one preceded by its own indicator. Integers follow the
//! representation; decimals are packed, numerics zoned; strings are
//! fixed or carry a two-byte length; dates, times and timestamps are
//! ISO characters.

// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

use qdecimal::{Decimal, Packed, Rounding, Zoned};

use crate::Error;
use crate::fdoca::desc::Field;
use crate::fdoca::types::drda;
use crate::fdoca::{Reader, Writer, sqlca};
use crate::sql::{Sqlca, Value};

/// A null indicator: present.
pub const PRESENT: u8 = 0x00;
/// A null indicator: null.
pub const NULL: u8 = 0xFF;

/// Write one value (its indicator included when nullable).
pub fn write_value(w: &mut Writer, f: &Field, v: &Value) -> Result<(), Error> {
    if f.nullable() {
        if matches!(v, Value::Null) {
            w.u8(NULL);
            return Ok(());
        }
        w.u8(PRESENT);
    } else if matches!(v, Value::Null) {
        return Err(Error::Invalid("a null value in a column that is not nullable".into()));
    }
    let bad = |e: qdecimal::DecimalError| Error::Invalid(e.to_string());
    let base = u16::from(f.drda & !1);
    let (p, s) = (u8::try_from(f.len >> 8).unwrap_or(0), u8::try_from(f.len & 0xFF).unwrap_or(0));
    match base {
        drda::SMALL => w.i16(i16::try_from(int_of(v)?).map_err(|_| overflow())?),
        drda::INTEGER => w.i32(i32::try_from(int_of(v)?).map_err(|_| overflow())?),
        drda::INTEGER8 => w.i64(int_of(v)?),
        drda::FLOAT8 => {
            let x = match v {
                Value::Double(x) => *x,
                other => other.render().parse().map_err(|_| overflow())?,
            };
            if w.repr.little {
                w.bytes(&x.to_le_bytes());
            } else {
                w.bytes(&x.to_be_bytes());
            }
        }
        drda::DECIMAL => {
            let d = dec_of(v)?;
            w.bytes(&Packed::new(p, s).map_err(bad)?.encode(&d, Rounding::Truncate).map_err(bad)?);
        }
        drda::ZDECIMAL => {
            let d = dec_of(v)?;
            w.bytes(&Zoned::new(p, s).map_err(bad)?.encode(&d, Rounding::Truncate).map_err(bad)?);
        }
        drda::CHAR | drda::MIX | drda::DATE | drda::TIME | drda::TIMESTAMP => {
            w.fixed(&v.render(), usize::from(f.len));
        }
        drda::VARCHAR | drda::VARMIX | drda::LONG | drda::LONGMIX => {
            let mut b = w.chars(&v.render());
            b.truncate(usize::from(f.len));
            w.be16(u16::try_from(b.len()).unwrap_or(0));
            w.bytes(&b);
        }
        other => return Err(Error::Invalid(format!("DRDA type {other:#04x}"))),
    }
    Ok(())
}

fn overflow() -> Error {
    Error::Invalid("a value out of the column's range".into())
}

fn int_of(v: &Value) -> Result<i64, Error> {
    match v {
        Value::Int(i) => Ok(*i),
        Value::Dec(d) => d.to_string().split('.').next().unwrap_or("0").parse().map_err(|_| overflow()),
        #[allow(clippy::cast_possible_truncation)] // the fraction goes, as SQL's CAST
        Value::Double(f) => Ok(f.trunc() as i64),
        Value::Text(t) => t.trim().parse().map_err(|_| overflow()),
        Value::Null => Err(overflow()),
    }
}

fn dec_of(v: &Value) -> Result<Decimal, Error> {
    match v {
        Value::Dec(d) => Ok(d.clone()),
        other => Decimal::parse(other.render().trim()).map_err(|e| Error::Invalid(e.to_string())),
    }
}

/// Read one value.
pub fn read_value(r: &mut Reader<'_>, f: &Field) -> Result<Value, Error> {
    if f.nullable() && r.u8()? != PRESENT {
        return Ok(Value::Null);
    }
    let bad = |e: qdecimal::DecimalError| Error::Invalid(e.to_string());
    let base = u16::from(f.drda & !1);
    let (p, s) = (u8::try_from(f.len >> 8).unwrap_or(0), u8::try_from(f.len & 0xFF).unwrap_or(0));
    Ok(match base {
        drda::SMALL => Value::Int(i64::from(r.i16()?)),
        drda::INTEGER => Value::Int(i64::from(r.i32()?)),
        drda::INTEGER8 => Value::Int(r.i64()?),
        drda::FLOAT8 => {
            let b: [u8; 8] = r.take(8)?.try_into().expect("8");
            Value::Double(if r.repr.little {
                f64::from_le_bytes(b)
            } else {
                f64::from_be_bytes(b)
            })
        }
        drda::DECIMAL => {
            let k = Packed::new(p, s).map_err(bad)?;
            Value::Dec(k.decode(r.take(k.byte_len())?).map_err(bad)?)
        }
        drda::ZDECIMAL => {
            let k = Zoned::new(p, s).map_err(bad)?;
            Value::Dec(k.decode(r.take(k.byte_len())?).map_err(bad)?)
        }
        drda::CHAR | drda::MIX | drda::DATE | drda::TIME | drda::TIMESTAMP => {
            let b = r.take(usize::from(f.len))?;
            let t = text_in(r, *f, b);
            Value::Text(if base == drda::CHAR || base == drda::MIX {
                t
            } else {
                t.trim_end().to_string()
            })
        }
        drda::VARCHAR | drda::VARMIX | drda::LONG | drda::LONGMIX => {
            let n = usize::from(r.be16()?);
            let b = r.take(n)?;
            Value::Text(text_in(r, *f, b))
        }
        drda::FIXBYTE => Value::Text(hex(r.take(usize::from(f.len))?)),
        drda::VARBYTE => {
            let n = usize::from(r.be16()?);
            Value::Text(hex(r.take(n)?))
        }
        other => return Err(Error::Invalid(format!("DRDA type {other:#04x}"))),
    })
}

/// Characters in the field's CCSID (the connection's when it has none).
fn text_in(r: &Reader<'_>, f: Field, b: &[u8]) -> String {
    match f.ccsid {
        0 => r.text(b),
        65535 => hex(b),
        c => ccsid::conv::to_string(c, b).unwrap_or_else(|_| r.text(b)),
    }
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// Write one answer set row: the null SQLCA, the group indicator and
/// the columns.
pub fn write_row(w: &mut Writer, fields: &[Field], row: &[Value]) -> Result<(), Error> {
    w.u8(sqlca::NULL);
    w.u8(PRESENT);
    for (f, v) in fields.iter().zip(row) {
        write_value(w, f, v)?;
    }
    Ok(())
}

/// Write the row that ends the data: the SQLCA (`+100` or an error)
/// and a null column group.
pub fn write_end(w: &mut Writer, ca: &Sqlca, sqlam: u16) {
    sqlca::write(w, Some(ca), sqlam);
    w.u8(NULL);
}

/// What one row of `QRYDTA` held.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// Column values.
    Values(Vec<Value>),
    /// The SQLCA that ends the data (`+100` or an error).
    End(Sqlca),
}

/// Read the rows of a `QRYDTA`.
pub fn read_rows(r: &mut Reader<'_>, fields: &[Field], sqlam: u16) -> Result<Vec<Row>, Error> {
    let mut out = Vec::new();
    while !r.done() {
        let ca = sqlca::read(r, sqlam)?;
        let group = r.u8()?;
        if group == NULL {
            out.push(Row::End(ca.unwrap_or_else(Sqlca::no_data)));
            continue;
        }
        let mut row = Vec::with_capacity(fields.len());
        for f in fields {
            row.push(read_value(r, f)?);
        }
        match ca {
            Some(ca) if ca.sqlcode != 0 && ca.is_error() => out.push(Row::End(ca)),
            _ => out.push(Row::Values(row)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fdoca::Repr;
    use crate::sql::{ColType, Kind};

    #[test]
    fn rows_round_trip() {
        let cols = [
            ColType::new(Kind::Integer, 0, 10, 0, false),
            ColType::new(Kind::Decimal, 0, 7, 2, true),
            ColType::new(Kind::VarChar, 20, 0, 0, true),
            ColType::new(Kind::Char, 4, 0, 0, false),
            ColType::new(Kind::Date, 0, 0, 0, true),
            ColType::new(Kind::BigInt, 0, 0, 0, false),
            ColType::new(Kind::Numeric, 0, 5, 0, false),
        ];
        let f: Vec<Field> = cols.iter().map(Field::of).collect();
        let row = vec![
            Value::Int(-7),
            Value::Dec(Decimal::parse("123.45").unwrap()),
            Value::Null,
            Value::Text("ab  ".into()),
            Value::Text("2026-10-29".into()),
            Value::Int(1 << 40),
            Value::Dec(Decimal::parse("-42").unwrap()),
        ];
        let mut w = Writer::new(Repr::default());
        write_row(&mut w, &f, &row).unwrap();
        write_end(&mut w, &Sqlca::no_data(), 7);
        let mut r = Reader::new(&w.out, Repr::default());
        let rows = read_rows(&mut r, &f, 7).unwrap();
        assert_eq!(rows.len(), 2);
        let Row::Values(v) = &rows[0] else { panic!() };
        assert_eq!(v[0], Value::Int(-7));
        assert_eq!(v[1].render(), "123.45");
        assert_eq!(v[2], Value::Null);
        assert_eq!(v[3], Value::Text("ab  ".into()));
        assert_eq!(v[6].render(), "-42");
        assert!(matches!(&rows[1], Row::End(ca) if ca.sqlcode == 100));
    }
}
