// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Literals as the bytes a type holds: initializers and constants.

use qdecimal::{Binary, Decimal, Float, Integer, Packed, Rounding, Zoned};

use super::Const;
use crate::types::{Type, TypeClass};

/// Encode a literal as the bytes of a type. The result is exactly the
/// type's size.
pub fn encode(ty: &Type, c: &Const) -> Result<Vec<u8>, String> {
    let size = usize::try_from(ty.size()).map_err(|_| "type too large".to_string())?;
    match c {
        Const::Hex(bytes) => {
            if bytes.len() > size {
                return Err(format!("{} bytes do not fit {ty}", bytes.len()));
            }
            let mut out = bytes.clone();
            out.resize(size, 0);
            Ok(out)
        }
        Const::Zeros => Ok(zeros(ty, size)),
        Const::Blanks => match ty.class() {
            TypeClass::Char
            | TypeClass::DateTime
            | TypeClass::Aggregate
            | TypeClass::Ucs2
            | TypeClass::Graphic => Ok(blanks(ty, size)),
            _ => Err(format!("*BLANKS is not a {ty}")),
        },
        Const::HiVal => hival(ty, size),
        Const::LoVal => loval(ty, size),
        Const::Null => match ty {
            Type::Ptr | Type::ProcPtr => Ok(vec![0; size]),
            _ => Err(format!("*NULL is not a {ty}")),
        },
        Const::On | Const::Off => match ty {
            Type::Ind => Ok(vec![if matches!(c, Const::On) { 0xF1 } else { 0xF0 }]),
            Type::Char(n) => Ok(vec![
                if matches!(c, Const::On) { 0xF1 } else { 0xF0 };
                *n as usize
            ]),
            _ => Err(format!("*ON and *OFF are not a {ty}")),
        },
        Const::Int(v) => encode_number(
            ty,
            &Decimal::new(i128_fit(*v)?, 0).map_err(|e| e.to_string())?,
        ),
        Const::Dec(text) => encode_number(ty, &Decimal::parse(text).map_err(|e| e.to_string())?),
        Const::Float(f) => match ty {
            Type::F32 => {
                #[allow(clippy::cast_possible_truncation)]
                let v = *f as f32;
                Ok(v.to_be_bytes().to_vec())
            }
            Type::F64 => Ok(f.to_be_bytes().to_vec()),
            _ => encode_number(
                ty,
                &Decimal::parse(&format!("{f}")).map_err(|e| e.to_string())?,
            ),
        },
        Const::Str(s) => encode_text(ty, s, size),
    }
}

fn i128_fit(v: i128) -> Result<i128, String> {
    if v.unsigned_abs() >= 10u128.pow(38) {
        return Err("integer literal has more than 38 digits".into());
    }
    Ok(v)
}

fn zeros(ty: &Type, size: usize) -> Vec<u8> {
    match ty {
        Type::Packed(s) => Packed::new(s.digits, s.scale)
            .and_then(|p| p.encode(&Decimal::zero(), Rounding::Truncate))
            .unwrap_or_else(|_| vec![0; size]),
        Type::Zoned(_) | Type::Ind | Type::Char(_) => vec![0xF0; size],
        Type::Struct(_) | Type::Array { .. } => {
            let mut out = vec![0; size];
            fill_default(ty, &mut out, true);
            out
        }
        _ => vec![0; size],
    }
}

fn blanks(ty: &Type, size: usize) -> Vec<u8> {
    match ty {
        Type::VarChar(..) | Type::VarUcs2(..) | Type::VarGraphic(..) => vec![0; size],
        Type::Ucs2(_) | Type::Graphic(_) => [0x00, 0x20].repeat(size / 2),
        Type::Struct(_) | Type::Array { .. } => {
            let mut out = vec![ccsid::SPACE; size];
            fill_default(ty, &mut out, false);
            out
        }
        _ => vec![ccsid::SPACE; size],
    }
}

/// Fill the numeric and pointer parts of an aggregate with their own
/// defaults, leaving text as blanks (or `0xF0` under `*ZEROS`).
fn fill_default(ty: &Type, out: &mut [u8], zeros_text: bool) {
    match ty {
        Type::Struct(s) => {
            for f in &s.fields {
                let start = f.offset as usize;
                let end = start + usize::try_from(f.ty.size()).expect("checked size");
                fill_default(&f.ty, &mut out[start..end], zeros_text);
            }
        }
        Type::Array { len, elem, .. } => {
            let n = usize::try_from(elem.size()).expect("checked size");
            let s = usize::try_from(ty.stride().unwrap_or(0)).expect("checked size");
            for i in 0..*len as usize {
                fill_default(elem, &mut out[i * s..i * s + n], zeros_text);
            }
        }
        Type::Char(_)
        | Type::Ucs2(_)
        | Type::Graphic(_)
        | Type::Date(_)
        | Type::Time(_)
        | Type::Timestamp(_)
        | Type::Bytes(_) => {}
        _ => {
            let d = default_bytes(ty, out.len(), zeros_text);
            out.copy_from_slice(&d);
        }
    }
}

/// The bytes a type holds when nothing initializes it.
#[must_use]
pub fn default_bytes(ty: &Type, size: usize, zeros_text: bool) -> Vec<u8> {
    match ty.class() {
        TypeClass::Char if zeros_text => vec![0xF0; size],
        TypeClass::Char | TypeClass::DateTime | TypeClass::Ucs2 | TypeClass::Graphic => {
            blanks(ty, size)
        }
        TypeClass::Aggregate => {
            let mut out = if zeros_text {
                vec![0xF0; size]
            } else {
                vec![ccsid::SPACE; size]
            };
            fill_default(ty, &mut out, zeros_text);
            out
        }
        _ => zeros(ty, size),
    }
}

fn hival(ty: &Type, size: usize) -> Result<Vec<u8>, String> {
    Ok(match ty {
        Type::I8 => vec![0x7F],
        Type::I16 => i16::MAX.to_be_bytes().to_vec(),
        Type::I32 => i32::MAX.to_be_bytes().to_vec(),
        Type::I64 => i64::MAX.to_be_bytes().to_vec(),
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Char(_)
        | Type::Bytes(_)
        | Type::Ucs2(_)
        | Type::Graphic(_) => vec![0xFF; size],
        Type::F32 => f32::MAX.to_be_bytes().to_vec(),
        Type::F64 => f64::MAX.to_be_bytes().to_vec(),
        Type::Packed(s) => {
            let mut out = vec![0x99; size];
            if s.digits % 2 == 0 {
                out[0] = 0x09;
            }
            out[size - 1] = 0x9F;
            out
        }
        Type::Zoned(_) => vec![0xF9; size],
        Type::Bin(s) => bin_extreme(*s, false)?,
        Type::Ind => vec![0xF1],
        _ => return Err(format!("*HIVAL is not a {ty}")),
    })
}

/// A binary decimal's largest (or smallest) value: all nines.
fn bin_extreme(s: crate::types::Shape, low: bool) -> Result<Vec<u8>, String> {
    let nines = "9".repeat(usize::from(s.digits));
    let text = if s.scale > 0 {
        let (i, f) = nines.split_at(usize::from(s.digits - s.scale));
        format!("{}{i}.{f}", if low { "-" } else { "" })
    } else {
        format!("{}{nines}", if low { "-" } else { "" })
    };
    let v = Decimal::parse(&text).map_err(|e| e.to_string())?;
    Binary::new(s.digits, s.scale)
        .and_then(|b| b.encode(&v, Rounding::Truncate))
        .map_err(|e| e.to_string())
}

fn loval(ty: &Type, size: usize) -> Result<Vec<u8>, String> {
    Ok(match ty {
        Type::I8 => vec![0x80],
        Type::I16 => i16::MIN.to_be_bytes().to_vec(),
        Type::I32 => i32::MIN.to_be_bytes().to_vec(),
        Type::I64 => i64::MIN.to_be_bytes().to_vec(),
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Char(_)
        | Type::Bytes(_)
        | Type::Ucs2(_)
        | Type::Graphic(_) => vec![0; size],
        Type::F32 => f32::MIN.to_be_bytes().to_vec(),
        Type::F64 => f64::MIN.to_be_bytes().to_vec(),
        Type::Packed(s) => {
            let mut out = vec![0x99; size];
            if s.digits % 2 == 0 {
                out[0] = 0x09;
            }
            out[size - 1] = 0x9D;
            out
        }
        Type::Zoned(_) => {
            let mut out = vec![0xF9; size];
            out[size - 1] = 0xD9;
            out
        }
        Type::Bin(s) => bin_extreme(*s, true)?,
        Type::Ind => vec![0xF0],
        _ => return Err(format!("*LOVAL is not a {ty}")),
    })
}

fn encode_number(ty: &Type, v: &Decimal) -> Result<Vec<u8>, String> {
    let r = match ty {
        Type::I8 => Integer::new(1, true).and_then(|i| i.encode(v)),
        Type::I16 => Integer::new(2, true).and_then(|i| i.encode(v)),
        Type::I32 => Integer::new(4, true).and_then(|i| i.encode(v)),
        Type::I64 => Integer::new(8, true).and_then(|i| i.encode(v)),
        Type::U8 => Integer::new(1, false).and_then(|i| i.encode(v)),
        Type::U16 => Integer::new(2, false).and_then(|i| i.encode(v)),
        Type::U32 => Integer::new(4, false).and_then(|i| i.encode(v)),
        Type::U64 => Integer::new(8, false).and_then(|i| i.encode(v)),
        Type::F32 => Float::SINGLE.encode(v),
        Type::F64 => Float::DOUBLE.encode(v),
        Type::Packed(s) => {
            Packed::new(s.digits, s.scale).and_then(|p| p.encode(v, Rounding::Truncate))
        }
        Type::Zoned(s) => {
            Zoned::new(s.digits, s.scale).and_then(|p| p.encode(v, Rounding::Truncate))
        }
        Type::Bin(s) => {
            Binary::new(s.digits, s.scale).and_then(|p| p.encode(v, Rounding::Truncate))
        }
        Type::Ind => {
            return match v.to_i128() {
                Some(0) => Ok(vec![0xF0]),
                Some(1) => Ok(vec![0xF1]),
                _ => Err("an indicator is 0 or 1".into()),
            };
        }
        _ => return Err(format!("a number is not a {ty}")),
    };
    r.map_err(|e| e.to_string())
}

fn encode_text(ty: &Type, s: &str, size: usize) -> Result<Vec<u8>, String> {
    match ty {
        Type::Char(_) | Type::Date(_) | Type::Time(_) | Type::Timestamp(_) | Type::Bytes(_) => {
            let mut out = vec![0; size];
            ccsid::encode_fixed(s, &mut out).map_err(|e| e.to_string())?;
            Ok(out)
        }
        Type::VarChar(n, p) => {
            let bytes = ccsid::to_ebcdic(s).map_err(|e| e.to_string())?;
            if bytes.len() > *n as usize {
                return Err(format!("{} characters do not fit {ty}", bytes.len()));
            }
            let mut out = vec![0; size];
            let at = put_var_len(&mut out, *p, bytes.len())?;
            out[at..at + bytes.len()].copy_from_slice(&bytes);
            Ok(out)
        }
        Type::Ucs2(n) | Type::Graphic(n) => {
            let units: Vec<u16> = s.encode_utf16().collect();
            if units.len() > *n as usize {
                return Err(format!("{} characters do not fit {ty}", units.len()));
            }
            let mut out = Vec::with_capacity(size);
            for u in &units {
                out.extend_from_slice(&u.to_be_bytes());
            }
            while out.len() < size {
                out.extend_from_slice(&[0x00, 0x20]);
            }
            Ok(out)
        }
        Type::VarUcs2(n, p) | Type::VarGraphic(n, p) => {
            let units: Vec<u16> = s.encode_utf16().collect();
            if units.len() > *n as usize {
                return Err(format!("{} characters do not fit {ty}", units.len()));
            }
            let mut out = vec![0; size];
            let mut at = put_var_len(&mut out, *p, units.len())?;
            for u in &units {
                out[at..at + 2].copy_from_slice(&u.to_be_bytes());
                at += 2;
            }
            Ok(out)
        }
        Type::Ind => match s {
            "0" => Ok(vec![0xF0]),
            "1" => Ok(vec![0xF1]),
            _ => Err("an indicator is '0' or '1'".into()),
        },
        _ if ty.is_numeric() => encode(ty, &Const::Dec(s.trim().to_string())),
        _ => Err(format!("text is not a {ty}")),
    }
}

/// Write a varying length into its 2- or 4-byte prefix; the data's
/// offset.
fn put_var_len(out: &mut [u8], prefix: u8, len: usize) -> Result<usize, String> {
    if prefix == 4 {
        let n = u32::try_from(len).map_err(|_| "too long".to_string())?;
        out[..4].copy_from_slice(&n.to_be_bytes());
        Ok(4)
    } else {
        let n = u16::try_from(len).map_err(|_| "too long".to_string())?;
        out[..2].copy_from_slice(&n.to_be_bytes());
        Ok(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Shape;

    #[test]
    fn numbers_take_the_machine_encoding() {
        let p = Type::Packed(Shape::new(5, 2).unwrap());
        assert_eq!(
            encode(&p, &Const::Dec("123.45".into())).unwrap(),
            vec![0x12, 0x34, 0x5C]
        );
        assert_eq!(encode(&p, &Const::Int(-1)).unwrap(), vec![0x00, 0x10, 0x0D]);
        assert_eq!(
            encode(&Type::I16, &Const::Int(-2)).unwrap(),
            vec![0xFF, 0xFE]
        );
        assert_eq!(
            encode(&Type::Zoned(Shape::new(3, 0).unwrap()), &Const::Int(7)).unwrap(),
            vec![0xF0, 0xF0, 0xC7]
        );
        assert!(encode(&Type::I8, &Const::Int(300)).is_err());
    }

    #[test]
    fn opaque_bytes_default_without_recursing() {
        assert_eq!(default_bytes(&Type::Bytes(3), 3, false), vec![0x40; 3]);
        assert_eq!(default_bytes(&Type::Bytes(2), 2, true), vec![0xF0; 2]);
    }

    #[test]
    fn long_varying_takes_a_4_byte_length() {
        assert_eq!(
            encode(&Type::VarChar(70_000, 4), &Const::Str("AB".into())).unwrap()[..6],
            [0, 0, 0, 2, 0xC1, 0xC2]
        );
        assert_eq!(
            encode(&Type::VarGraphic(3, 2), &Const::Str("A".into())).unwrap(),
            vec![0, 1, 0, 0x41, 0, 0, 0, 0]
        );
    }

    #[test]
    fn text_is_cp037_padded() {
        assert_eq!(
            encode(&Type::Char(4), &Const::Str("AB".into())).unwrap(),
            vec![0xC1, 0xC2, 0x40, 0x40]
        );
        assert_eq!(
            encode(&Type::VarChar(4, 2), &Const::Str("AB".into())).unwrap(),
            vec![0, 2, 0xC1, 0xC2, 0, 0]
        );
        assert_eq!(
            encode(&Type::Ucs2(2), &Const::Str("A".into())).unwrap(),
            vec![0, 0x41, 0, 0x20]
        );
        assert!(encode(&Type::Char(1), &Const::Str("AB".into())).is_err());
    }

    #[test]
    fn figurative_constants() {
        assert_eq!(
            encode(&Type::Char(2), &Const::Blanks).unwrap(),
            vec![0x40, 0x40]
        );
        assert_eq!(encode(&Type::Ind, &Const::On).unwrap(), vec![0xF1]);
        assert_eq!(encode(&Type::Ptr, &Const::Null).unwrap(), vec![0; 16]);
        let p = Type::Packed(Shape::new(4, 0).unwrap());
        assert_eq!(encode(&p, &Const::HiVal).unwrap(), vec![0x09, 0x99, 0x9F]);
        assert_eq!(encode(&p, &Const::LoVal).unwrap(), vec![0x09, 0x99, 0x9D]);
        assert_eq!(
            encode(&Type::Char(3), &Const::Hex(vec![0xC1])).unwrap(),
            vec![0xC1, 0, 0]
        );
    }

    #[test]
    fn struct_default_mixes_blanks_and_zeros() {
        let s = crate::types::StructType::new(
            vec![
                ("a".into(), None, Type::Char(2)),
                ("n".into(), None, Type::Packed(Shape::new(3, 0).unwrap())),
            ],
            None,
        )
        .unwrap();
        let t = Type::Struct(Box::new(s));
        assert_eq!(
            encode(&t, &Const::Blanks).unwrap(),
            vec![0x40, 0x40, 0x00, 0x0C]
        );
        assert_eq!(default_bytes(&t, 4, false), vec![0x40, 0x40, 0x00, 0x0C]);
    }
}
