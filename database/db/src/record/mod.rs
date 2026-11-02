// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Field values in a record buffer: reading them out by type, writing
//! them in with conversion, and comparing them as the machine does.

use std::cmp::Ordering;

use dds::{DataType, FieldDef, FloatPrecision, Literal};
use qdecimal::{Binary, DecFloat, Decimal, DfFormat, Float, Integer, Packed, Rounding, Zoned};

use crate::error::{DbError, Result};

pub mod datetime;

/// A field's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// The field is null.
    Null,
    /// A zoned, packed or binary field.
    Num(Decimal),
    /// A floating point field, a single widened.
    Float(f64),
    /// Character, hexadecimal, graphic, DBCS-open, date, time or
    /// timestamp bytes, CP037 for text, without `VARLEN` padding; the
    /// bytes of a binary string or a `BOOLEAN`; a LOB's `LobRef`.
    Char(Vec<u8>),
    /// A `DECFLOAT` field.
    DecFloat(DecFloat),
}

// Floats compare by value; a NaN is never written by the database.
impl Eq for Value {}

/// A finite float as a decimal, its shortest text that reads back the
/// same; `None` past 63 digits or for a special value.
#[must_use]
pub fn float_decimal(x: f64) -> Option<Decimal> {
    x.is_finite().then(|| Decimal::parse(&format!("{x}")).ok()).flatten()
}

/// A float's shortest text that reads back the same, in E notation
/// (`1.5E-30`), or `Infinity`, `-Infinity`, `NaN`.
#[must_use]
pub fn float_text(x: f64) -> String {
    if x.is_nan() {
        "NaN".into()
    } else if x.is_infinite() {
        if x < 0.0 { "-Infinity" } else { "Infinity" }.into()
    } else {
        format!("{x:E}")
    }
}

/// A float as it is shown: plain digits when its exponent is -7 to
/// 20 (`4.5`, `0.00001`), else [`float_text`] (`1E300`).
#[must_use]
pub fn float_display(x: f64) -> String {
    if x.is_finite() && (x == 0.0 || (1e-7..1e21).contains(&x.abs())) {
        format!("{x}")
    } else {
        float_text(x)
    }
}

/// A single as it is shown, its own shortest digits, as
/// [`float_display`] shows a double.
#[must_use]
pub fn single_display(x: f32) -> String {
    if x.is_finite() && (x == 0.0 || (1e-7..1e21).contains(&f64::from(x).abs())) {
        format!("{x}")
    } else if x.is_finite() {
        format!("{x:E}")
    } else {
        float_text(f64::from(x))
    }
}

/// A float field's value as it is shown.
#[must_use]
pub fn float_shown(f: &FieldDef, x: f64) -> String {
    #[allow(clippy::cast_possible_truncation)] // a single, widened when read
    if f.precision == FloatPrecision::Single { single_display(x as f32) } else { float_display(x) }
}

/// A decimal as the nearest double.
#[must_use]
pub fn decimal_f64(d: &Decimal) -> f64 {
    d.to_string().parse().expect("decimal text parses as a float")
}

/// The format of a `DECFLOAT` field.
#[must_use]
pub fn decfloat_format(f: &FieldDef) -> DfFormat {
    if f.length <= 16 { DfFormat::D16 } else { DfFormat::D34 }
}

/// What a byte field is padded with: 0x00 for binary strings, blanks
/// for text.
fn pad_byte(f: &FieldDef) -> u8 {
    if matches!(f.data_type, DataType::Bytes | DataType::Boolean) || f.data_type.is_lob() {
        0
    } else {
        ccsid::SPACE
    }
}

/// A graphic field of CCSID 1200 or 13488, or none: UTF-16BE, two
/// bytes a character, padded with U+0020.
#[must_use]
pub fn is_utf16(f: &FieldDef) -> bool {
    f.data_type == DataType::Graphic && matches!(f.ccsid, None | Some(1200 | 13488))
}

/// UTF-16BE bytes as text.
#[must_use]
pub fn utf16_text(b: &[u8]) -> String {
    let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
    String::from_utf16_lossy(&units)
}

/// Fill with the field's padding: U+0020 for UTF-16, else its byte.
pub fn fill_pad(f: &FieldDef, out: &mut [u8]) {
    if is_utf16(f) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = if i % 2 == 0 { 0x00 } else { 0x20 };
        }
    } else {
        out.fill(pad_byte(f));
    }
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

/// Write a float field.
fn write_float(f: &FieldDef, out: &mut [u8], value: &Value) -> Result<()> {
    let x = match value {
        Value::Null => 0.0,
        Value::Float(x) => *x,
        Value::Num(d) => decimal_f64(d),
        Value::DecFloat(v) => v.to_f64(),
        Value::Char(_) => return Err(mapping(f, "bytes into a float field")),
    };
    let b = float_of(f).encode_f64(x).map_err(|e| mapping(f, e))?;
    out.copy_from_slice(&b);
    Ok(())
}

/// A float field's encoding.
fn float_of(f: &FieldDef) -> Float {
    match f.precision {
        FloatPrecision::Single => Float::SINGLE,
        FloatPrecision::Double => Float::DOUBLE,
    }
}

/// An SQL integer field's encoding: its bytes, signed.
fn int_of(f: &FieldDef) -> qdecimal::Result<Integer> {
    Integer::new(u8::try_from(f.data_len()).unwrap_or(8), true)
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
        // A UTF-16 field's prefix counts its characters.
        let unit = if is_utf16(f) { 2 } else { 1 };
        let len = (usize::from(u16::from_be_bytes([all[0], all[1]])) * unit).min(all.len() - 2);
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
        DataType::Binary if f.int_range => int_of(f).and_then(|i| i.decode(bytes)),
        DataType::Binary => Binary::new(digits(f), f.decimals).and_then(|b| b.decode(bytes)),
        DataType::Float => {
            return float_of(f).decode_f64(bytes).map(Value::Float).map_err(|e| mapping(f, e));
        }
        DataType::DecFloat => {
            return DecFloat::decode(bytes, decfloat_format(f))
                .map(Value::DecFloat)
                .map_err(|e| mapping(f, e));
        }
        _ => return Ok(Value::Char(bytes.to_vec())),
    };
    num.map(Value::Num).map_err(|e| mapping(f, e))
}

/// Write a value into its field, converting to the field's type: text
/// is blank padded or cut, numbers are fitted (truncated).
pub fn write_field(f: &FieldDef, record: &mut [u8], value: &Value) -> Result<()> {
    let off = usize::try_from(f.offset).expect("fits");
    let out = &mut record[off..off + usize::try_from(f.byte_len).expect("fits")];
    if f.data_type == DataType::DecFloat {
        let fmt = decfloat_format(f);
        let v = match value {
            Value::Null => DecFloat::zero(fmt),
            Value::DecFloat(v) => v.to_format(fmt).map_err(|e| mapping(f, e))?,
            Value::Num(d) => DecFloat::from_decimal(d, fmt).map_err(|e| mapping(f, e))?,
            Value::Float(x) => DecFloat::parse(&float_text(*x), fmt).map_err(|e| mapping(f, e))?,
            Value::Char(_) => return Err(mapping(f, "bytes into a DECFLOAT field")),
        };
        out.copy_from_slice(&v.encode());
        return Ok(());
    }
    if f.data_type == DataType::Boolean {
        out[0] = match value {
            Value::Null => 0xF0,
            Value::Char(b) if matches!(b.first(), Some(0xF0 | 0xF1)) => b[0],
            Value::Num(d) => if d.is_zero() { 0xF0 } else { 0xF1 },
            Value::Float(x) => if *x == 0.0 { 0xF0 } else { 0xF1 },
            _ => return Err(mapping(f, "not a BOOLEAN")),
        };
        return Ok(());
    }
    if f.data_type == DataType::Float {
        return write_float(f, out, value);
    }
    if let Value::Float(x) = value {
        if !f.data_type.is_numeric() {
            return Err(mapping(f, format!("number {x:e} into a character field")));
        }
        let d = float_decimal(*x).ok_or_else(|| mapping(f, format!("{x:e} is out of range")))?;
        return write_field(f, record, &Value::Num(d));
    }
    if let Value::DecFloat(v) = value {
        if !f.data_type.is_numeric() {
            return Err(mapping(f, format!("number {v} into a character field")));
        }
        let d = v.to_decimal().map_err(|e| mapping(f, e))?;
        return write_field(f, record, &Value::Num(d));
    }
    match (f.data_type.is_numeric(), value) {
        (_, Value::DecFloat(_) | Value::Float(_)) => unreachable!("handled above"),
        (_, Value::Null) => {
            // The null map says null; the bytes hold the type's zero.
            if f.data_type.is_numeric() {
                return write_field(f, record, &Value::Num(Decimal::zero()));
            }
            fill_pad(f, out);
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
                DataType::Binary if f.int_range => d
                    .rescale(0, r)
                    .and_then(|whole| int_of(f)?.encode(&whole))
                    .map(|b| out.copy_from_slice(&b)),
                _ => Binary::new(digits(f), f.decimals).and_then(|b| b.encode_into(d, r, out)),
            };
            res.map_err(|e| mapping(f, e))
        }
        (false, Value::Char(bytes)) => {
            // A UTF-16 field takes whole characters.
            let unit = if is_utf16(f) { 2 } else { 1 };
            if f.varlen.is_some() {
                let max = out.len() - 2;
                let n = bytes.len().min(max) / unit * unit;
                out[..2].copy_from_slice(&u16::try_from(n / unit).expect("fits").to_be_bytes());
                out[2..2 + n].copy_from_slice(&bytes[..n]);
                fill_pad(f, &mut out[2 + n..]);
            } else {
                let n = bytes.len().min(out.len()) / unit * unit;
                out[..n].copy_from_slice(&bytes[..n]);
                fill_pad(f, &mut out[n..]);
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
        (Value::Null, _) | (Value::Char(_), Value::Num(_) | Value::DecFloat(_) | Value::Float(_)) => {
            Ordering::Greater
        }
        (_, Value::Null) | (Value::Num(_) | Value::DecFloat(_) | Value::Float(_), Value::Char(_)) => Ordering::Less,
        (Value::Num(x), Value::Num(y)) => x.cmp(y),
        (Value::Float(x), Value::Float(y)) => fcmp(*x, *y),
        (Value::Float(x), Value::Num(y)) => fcmp(*x, decimal_f64(y)),
        (Value::Num(x), Value::Float(y)) => fcmp(decimal_f64(x), *y),
        (Value::Float(x), Value::DecFloat(y)) => fcmp(*x, y.to_f64()),
        (Value::DecFloat(x), Value::Float(y)) => fcmp(x.to_f64(), *y),
        (Value::DecFloat(x), Value::DecFloat(y)) => x.sql_cmp(y),
        (Value::DecFloat(x), Value::Num(y)) => {
            DecFloat::from_decimal(y, DfFormat::D34).map_or(Ordering::Greater, |y| x.sql_cmp(&y))
        }
        (Value::Num(x), Value::DecFloat(y)) => {
            DecFloat::from_decimal(x, DfFormat::D34).map_or(Ordering::Less, |x| x.sql_cmp(y))
        }
        (Value::Char(x), Value::Char(y)) => ccsid::ebcdic_cmp(x, y),
    }
}

/// Floats by value, zeros alike; a NaN after every number.
#[must_use]
pub fn fcmp(x: f64, y: f64) -> Ordering {
    x.partial_cmp(&y).unwrap_or_else(|| x.is_nan().cmp(&y.is_nan()))
}

/// A DDS literal as a value of field `f`.
pub fn literal_value(f: &FieldDef, lit: &Literal) -> Result<Value> {
    Ok(match lit {
        Literal::Quoted(q) if is_utf16(f) => Value::Char(q.encode_utf16().flat_map(u16::to_be_bytes).collect()),
        Literal::Quoted(q) => Value::Char(
            q.chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect(),
        ),
        Literal::Hex(h) => Value::Char(h.clone()),
        Literal::Word(w) if f.data_type == DataType::DecFloat => Value::DecFloat(
            DecFloat::parse(w, decfloat_format(f)).map_err(|e| mapping(f, e))?,
        ),
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
    fn sql_only_types() {
        let mut rec = vec![0u8; 40];
        let df = f(DataType::DecFloat, 16, 0, 0);
        assert_eq!(df.byte_len, 8);
        let v = Value::DecFloat(DecFloat::parse("-1.50E+3", DfFormat::D16).unwrap());
        write_field(&df, &mut rec, &v).unwrap();
        assert_eq!(read_field(&df, &rec, &[], None).unwrap(), v);
        write_field(&df, &mut rec, &Value::Num(Decimal::parse("2.5").unwrap())).unwrap();
        assert_eq!(
            compare(&read_field(&df, &rec, &[], None).unwrap(), &Value::Num(Decimal::parse("2.50").unwrap())),
            Ordering::Equal
        );
        write_field(&df, &mut rec, &Value::Null).unwrap();
        assert_eq!(read_field(&df, &rec, &[], None).unwrap(), Value::DecFloat(DecFloat::zero(DfFormat::D16)));
        let b = f(DataType::Bytes, 4, 0, 8);
        write_field(&b, &mut rec, &Value::Char(vec![0x40, 0xFF])).unwrap();
        assert_eq!(&rec[8..12], &[0x40, 0xFF, 0, 0]);
        let t = f(DataType::Boolean, 1, 0, 12);
        write_field(&t, &mut rec, &Value::Char(vec![0xF1])).unwrap();
        assert_eq!(rec[12], 0xF1);
        write_field(&t, &mut rec, &Value::Null).unwrap();
        assert_eq!(rec[12], 0xF0);
        assert!(write_field(&t, &mut rec, &Value::Char(vec![0x40])).is_err());
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
