// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Field values in a record buffer: reading them out by type, writing
//! them in with conversion, and comparing them as the machine does.

use std::cmp::Ordering;

use dds::{DataType, FieldDef, FloatPrecision, Literal};
use qdecimal::{Binary, Decimal, Float, Packed, Rounding, Zoned};

use crate::error::{DbError, Result};

/// A field's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// The field is null.
    Null,
    /// A numeric field.
    Num(Decimal),
    /// Character, hexadecimal, graphic, DBCS-open, date, time or
    /// timestamp bytes, CP037 for text, without `VARLEN` padding.
    Char(Vec<u8>),
}

fn mapping(f: &FieldDef, what: impl std::fmt::Display) -> DbError {
    DbError::DataMapping {
        field: f.name.to_string(),
        what: what.to_string(),
    }
}

fn digits(f: &FieldDef) -> u8 {
    u8::try_from(f.length).expect("numeric length at most 63")
}

/// Whether bit `null_index` of the null map is set.
#[must_use]
pub fn is_null(null_map: &[u8], null_index: Option<u16>) -> bool {
    null_index.is_some_and(|i| {
        let i = usize::from(i);
        null_map.get(i / 8).is_some_and(|b| b & (1 << (i % 8)) != 0)
    })
}

/// Set or clear bit `null_index`.
pub fn set_null(null_map: &mut [u8], null_index: u16, null: bool) {
    let i = usize::from(null_index);
    if null {
        null_map[i / 8] |= 1 << (i % 8);
    } else {
        null_map[i / 8] &= !(1 << (i % 8));
    }
}

/// The field's bytes in the record: for a `VARLEN` field the bytes the
/// length prefix names, else the whole field.
#[must_use]
pub fn field_bytes<'r>(f: &FieldDef, record: &'r [u8]) -> &'r [u8] {
    let off = usize::try_from(f.offset).expect("fits");
    let all = &record[off..off + usize::try_from(f.byte_len).expect("fits")];
    if f.varlen.is_some() {
        let len = usize::from(u16::from_be_bytes([all[0], all[1]])).min(all.len() - 2);
        &all[2..2 + len]
    } else {
        all
    }
}

/// Read a field's value.
pub fn read_field(
    f: &FieldDef,
    record: &[u8],
    null_map: &[u8],
    null_index: Option<u16>,
) -> Result<Value> {
    if is_null(null_map, null_index) {
        return Ok(Value::Null);
    }
    let bytes = field_bytes(f, record);
    let num = match f.data_type {
        DataType::Zoned => Zoned::new(digits(f), f.decimals).and_then(|z| z.decode(bytes)),
        DataType::Packed => Packed::new(digits(f), f.decimals).and_then(|p| p.decode(bytes)),
        DataType::Binary => Binary::new(digits(f), f.decimals).and_then(|b| b.decode(bytes)),
        DataType::Float => match f.precision {
            FloatPrecision::Single => Float::SINGLE.decode(bytes),
            FloatPrecision::Double => Float::DOUBLE.decode(bytes),
        },
        _ => return Ok(Value::Char(bytes.to_vec())),
    };
    num.map(Value::Num).map_err(|e| mapping(f, e))
}

/// Write a value into its field, converting to the field's type: text
/// is blank padded or cut, numbers are fitted (truncated).
pub fn write_field(f: &FieldDef, record: &mut [u8], value: &Value) -> Result<()> {
    let off = usize::try_from(f.offset).expect("fits");
    let out = &mut record[off..off + usize::try_from(f.byte_len).expect("fits")];
    match (f.data_type.is_numeric(), value) {
        (_, Value::Null) => {
            // The null map says null; the bytes hold the type's zero.
            if f.data_type.is_numeric() {
                return write_field(f, record, &Value::Num(Decimal::zero()));
            }
            out.fill(ccsid::SPACE);
            if f.varlen.is_some() {
                out[..2].fill(0);
            }
            Ok(())
        }
        (true, Value::Num(d)) => {
            let r = Rounding::Truncate;
            let res = match f.data_type {
                DataType::Zoned => {
                    Zoned::new(digits(f), f.decimals).and_then(|z| z.encode_into(d, r, out))
                }
                DataType::Packed => {
                    Packed::new(digits(f), f.decimals).and_then(|p| p.encode_into(d, r, out))
                }
                DataType::Binary => {
                    Binary::new(digits(f), f.decimals).and_then(|b| b.encode_into(d, r, out))
                }
                _ => {
                    let fl = match f.precision {
                        FloatPrecision::Single => Float::SINGLE,
                        FloatPrecision::Double => Float::DOUBLE,
                    };
                    fl.encode(d).map(|b| out.copy_from_slice(&b))
                }
            };
            res.map_err(|e| mapping(f, e))
        }
        (false, Value::Char(bytes)) => {
            if f.varlen.is_some() {
                let max = out.len() - 2;
                let n = bytes.len().min(max);
                out[..2].copy_from_slice(&u16::try_from(n).expect("fits").to_be_bytes());
                out[2..2 + n].copy_from_slice(&bytes[..n]);
                out[2 + n..].fill(ccsid::SPACE);
            } else {
                let n = bytes.len().min(out.len());
                out[..n].copy_from_slice(&bytes[..n]);
                out[n..].fill(ccsid::SPACE);
            }
            Ok(())
        }
        (true, Value::Char(bytes)) => {
            // Zoned bytes into a numeric field: as a CONCAT of zoned parts gives.
            let z = Zoned::new(
                u8::try_from(bytes.len()).map_err(|_| mapping(f, "too long"))?,
                0,
            )
            .and_then(|z| z.decode(bytes))
            .map_err(|e| mapping(f, e))?;
            write_field(f, record, &Value::Num(z))
        }
        (false, Value::Num(d)) => Err(mapping(f, format!("number {d} into a character field"))),
    }
}

/// Compare as the machine does: nulls high, numbers by value, text by
/// EBCDIC byte order with blank padding.
#[must_use]
pub fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Null, _) | (Value::Char(_), Value::Num(_)) => Ordering::Greater,
        (_, Value::Null) | (Value::Num(_), Value::Char(_)) => Ordering::Less,
        (Value::Num(x), Value::Num(y)) => x.cmp(y),
        (Value::Char(x), Value::Char(y)) => ccsid::ebcdic_cmp(x, y),
    }
}

/// A DDS literal as a value of field `f`.
pub fn literal_value(f: &FieldDef, lit: &Literal) -> Result<Value> {
    Ok(match lit {
        Literal::Quoted(q) => Value::Char(
            q.chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect(),
        ),
        Literal::Hex(h) => Value::Char(h.clone()),
        Literal::Word(w) if f.data_type.is_numeric() => {
            Value::Num(Decimal::parse(w).map_err(|e| mapping(f, e))?)
        }
        Literal::Word(w) => Value::Char(
            w.chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsname::Name;

    fn f(t: DataType, len: u32, dec: u8, off: u32) -> FieldDef {
        let mut f = FieldDef::new(Name::parse("F").unwrap(), t, len, dec);
        f.offset = off;
        f
    }

    #[test]
    fn numbers_round_trip_and_convert() {
        let mut rec = vec![0u8; 20];
        let p = f(DataType::Packed, 7, 2, 0);
        write_field(
            &p,
            &mut rec,
            &Value::Num(Decimal::parse("-123.456").unwrap()),
        )
        .unwrap();
        assert_eq!(
            read_field(&p, &rec, &[], None).unwrap(),
            Value::Num(Decimal::parse("-123.45").unwrap())
        );
        let z = f(DataType::Zoned, 5, 0, 4);
        write_field(&z, &mut rec, &Value::Num(Decimal::parse("42").unwrap())).unwrap();
        assert_eq!(&rec[4..9], &[0xF0, 0xF0, 0xF0, 0xF4, 0xC2]);
        let b = f(DataType::Binary, 9, 0, 9);
        write_field(&b, &mut rec, &Value::Num(Decimal::parse("-7").unwrap())).unwrap();
        assert_eq!(
            read_field(&b, &rec, &[], None).unwrap(),
            Value::Num(Decimal::parse("-7").unwrap())
        );
        // Zoned bytes may go into a numeric field; blanks are not zoned.
        write_field(&z, &mut rec, &Value::Char(b"\xC1".to_vec())).unwrap();
        assert_eq!(
            read_field(&z, &rec, &[], None).unwrap(),
            Value::Num(Decimal::parse("1").unwrap())
        );
        assert!(write_field(&z, &mut rec, &Value::Char(b"\x40".to_vec())).is_err());
        rec[4] = 0x40;
        assert!(matches!(
            read_field(&z, &rec, &[], None),
            Err(DbError::DataMapping { .. })
        ));
    }

    #[test]
    fn text_varlen_and_nulls() {
        let mut rec = vec![0u8; 20];
        let a = f(DataType::Char, 5, 0, 0);
        write_field(&a, &mut rec, &Value::Char(b"\xC1\xC2".to_vec())).unwrap();
        assert_eq!(&rec[..5], &[0xC1, 0xC2, 0x40, 0x40, 0x40]);
        assert_eq!(
            read_field(&a, &rec, &[], None).unwrap(),
            Value::Char(vec![0xC1, 0xC2, 0x40, 0x40, 0x40])
        );
        let mut v = f(DataType::Char, 6, 0, 5);
        v.varlen = Some(dds::VarLen { allocated: None });
        v.byte_len = 8;
        write_field(&v, &mut rec, &Value::Char(b"\xC3\xC4\xC5".to_vec())).unwrap();
        assert_eq!(&rec[5..13], &[0, 3, 0xC3, 0xC4, 0xC5, 0x40, 0x40, 0x40]);
        assert_eq!(
            read_field(&v, &rec, &[], None).unwrap(),
            Value::Char(vec![0xC3, 0xC4, 0xC5])
        );
        let mut nulls = [0u8; 1];
        set_null(&mut nulls, 3, true);
        assert!(is_null(&nulls, Some(3)));
        assert!(!is_null(&nulls, Some(2)));
        assert!(!is_null(&nulls, None));
        assert_eq!(read_field(&a, &rec, &nulls, Some(3)).unwrap(), Value::Null);
        set_null(&mut nulls, 3, false);
        assert_eq!(nulls, [0]);
    }

    #[test]
    fn ordering_and_literals() {
        let n = |s: &str| Value::Num(Decimal::parse(s).unwrap());
        assert_eq!(compare(&n("-1"), &n("1")), Ordering::Less);
        assert_eq!(compare(&n("1.0"), &n("1")), Ordering::Equal);
        assert_eq!(
            compare(
                &Value::Char(b"\xC1".to_vec()),
                &Value::Char(b"\xC1\x40".to_vec())
            ),
            Ordering::Equal
        );
        assert_eq!(compare(&Value::Null, &n("9")), Ordering::Greater);
        let a = f(DataType::Char, 3, 0, 0);
        assert_eq!(
            literal_value(&a, &Literal::Quoted("AB".into())).unwrap(),
            Value::Char(vec![0xC1, 0xC2])
        );
        let p = f(DataType::Packed, 5, 2, 0);
        assert_eq!(
            literal_value(&p, &Literal::Word("12.5".into())).unwrap(),
            n("12.5")
        );
        assert!(literal_value(&p, &Literal::Word("X".into())).is_err());
    }
}
