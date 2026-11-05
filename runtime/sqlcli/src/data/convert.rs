// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Values to and from the program's buffers, by C type. Integers and
//! floats are big-endian, decimals packed (`SQL_DECIMAL`) or zoned
//! (`SQL_NUMERIC`) with the precision and scale the binding names,
//! characters in the job's CCSID (37).
//!
//! What the oracle settled for a column: character data that is
//! shorter than the buffer is followed by a NUL and its length word
//! says `SQL_NTS`; data that fills it or is longer fills it, without a
//! NUL, and the word holds the buffer's length (no warning); any other
//! type leaves 0 there. A null value writes blanks into a character
//! buffer and zero into a number, with -1 in the word; without a word
//! it is `22002`.

use qdecimal::{Decimal, Packed, Rounding, Zoned};
use sql::Value;

use crate::consts::{len, ty};
use crate::model::diag::Diag;

/// What goes into a buffer: its bytes and the length word's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// The bytes, from the buffer's start.
    pub bytes: Vec<u8>,
    /// The length word.
    pub ind: i32,
}

/// The C types a buffer may have.
#[must_use]
pub fn supported(ctype: i16) -> bool {
    matches!(
        ctype,
        ty::CHAR
            | ty::VARCHAR
            | ty::INTEGER
            | ty::SMALLINT
            | ty::BIGINT
            | ty::REAL
            | ty::FLOAT
            | ty::DOUBLE
            | ty::DECIMAL
            | ty::NUMERIC
            | ty::DATE
            | ty::TIME
            | ty::TIMESTAMP
            | ty::BINARY
            | ty::VARBINARY
    )
}

/// Whether the C type is character data.
#[must_use]
pub fn is_char(ctype: i16) -> bool {
    matches!(ctype, ty::CHAR | ty::DATE | ty::TIME | ty::TIMESTAMP)
}

/// The bytes a buffer of `ctype` takes: for decimals `max` is the
/// precision * 256 + scale.
#[must_use]
pub fn buffer_len(ctype: i16, max: i32) -> usize {
    let m = usize::try_from(max).unwrap_or(0);
    match ctype {
        ty::INTEGER | ty::REAL => 4,
        ty::SMALLINT => 2,
        ty::BIGINT | ty::DOUBLE | ty::FLOAT => 8,
        ty::DECIMAL => usize::try_from(max >> 8).unwrap_or(0) / 2 + 1,
        ty::NUMERIC => usize::try_from(max >> 8).unwrap_or(0),
        _ => m,
    }
}

fn out_of_range() -> Diag {
    Diag { state: "22003".into(), native: -302, text: "A value does not fit its buffer.".into() }
}

fn not_convertible() -> Diag {
    Diag { state: "22018".into(), native: -420, text: "A value cannot be converted to the buffer's type.".into() }
}

/// A decimal precision and scale from a binding's word.
fn shape(word: i32) -> Result<(u8, u8), Diag> {
    let p = u8::try_from(word >> 8).map_err(|_| out_of_range())?;
    let s = u8::try_from(word & 0xFF).map_err(|_| out_of_range())?;
    if p == 0 || p > 63 || s > p {
        return Err(Diag::cli("HY104", "The precision or scale of a decimal buffer is not valid."));
    }
    Ok((p, s))
}

/// A value's text as a character buffer holds it.
fn text_of(v: &Value) -> String {
    match v {
        Value::Bin(b) => ccsid::from_ebcdic(b),
        other => other.as_text(),
    }
}

fn int_image(v: &Value, bytes: usize) -> Result<Vec<u8>, Diag> {
    let d = v.as_decimal().map_err(|_| not_convertible())?;
    let d = d.rescale(0, Rounding::Truncate).map_err(|_| out_of_range())?;
    let n = d.to_i128().ok_or_else(out_of_range)?;
    Ok(match bytes {
        2 => i16::try_from(n).map_err(|_| out_of_range())?.to_be_bytes().to_vec(),
        4 => i32::try_from(n).map_err(|_| out_of_range())?.to_be_bytes().to_vec(),
        _ => i64::try_from(n).map_err(|_| out_of_range())?.to_be_bytes().to_vec(),
    })
}

fn dec_image(v: &Value, word: i32, zoned: bool) -> Result<Vec<u8>, Diag> {
    let (p, s) = shape(word)?;
    let d = v.as_decimal().map_err(|_| not_convertible())?;
    let d = d.rescale(s, Rounding::Truncate).map_err(|_| out_of_range())?;
    let r = if zoned {
        Zoned::new(p, s).and_then(|z| z.encode(&d, Rounding::Truncate))
    } else {
        Packed::new(p, s).and_then(|z| z.encode(&d, Rounding::Truncate))
    };
    r.map_err(|_| out_of_range())
}

/// The image of a value in a buffer of `ctype` and `max` bytes.
pub fn image(v: &Value, ctype: i16, max: i32, nts: bool) -> Result<Image, Diag> {
    let cap = usize::try_from(max).unwrap_or(0);
    let fixed = |bytes: Vec<u8>| Image { bytes, ind: 0 };
    Ok(match ctype {
        ty::CHAR | ty::DATE | ty::TIME | ty::TIMESTAMP => {
            let mut b = super::ebcdic(&text_of(v));
            if b.len() < cap {
                let n = b.len();
                if nts {
                    b.push(0);
                    Image { bytes: b, ind: len::NTS }
                } else {
                    Image { bytes: b, ind: i32::try_from(n).unwrap_or(i32::MAX) }
                }
            } else {
                b.truncate(cap);
                Image { bytes: b, ind: max }
            }
        }
        ty::VARCHAR | ty::VARBINARY => {
            let mut data = match v {
                Value::Bin(b) => b.clone(),
                other if ctype == ty::VARBINARY => other.as_bytes().map_err(|_| not_convertible())?,
                other => super::ebcdic(&text_of(other)),
            };
            data.truncate(cap.saturating_sub(2));
            let mut b = u16::try_from(data.len()).unwrap_or(0).to_be_bytes().to_vec();
            b.extend(data);
            fixed(b)
        }
        ty::BINARY => {
            let mut data = v.as_bytes().map_err(|_| not_convertible())?;
            let n = data.len();
            data.truncate(cap);
            Image { bytes: data, ind: i32::try_from(n.min(cap)).unwrap_or(0) }
        }
        ty::INTEGER => fixed(int_image(v, 4)?),
        ty::SMALLINT => fixed(int_image(v, 2)?),
        ty::BIGINT => fixed(int_image(v, 8)?),
        ty::DOUBLE | ty::FLOAT => fixed(v.as_f64().map_err(|_| not_convertible())?.to_be_bytes().to_vec()),
        #[allow(clippy::cast_possible_truncation)] // a REAL buffer
        ty::REAL => fixed((v.as_f64().map_err(|_| not_convertible())? as f32).to_be_bytes().to_vec()),
        ty::DECIMAL => fixed(dec_image(v, max, false)?),
        ty::NUMERIC => fixed(dec_image(v, max, true)?),
        _ => return Err(unsupported(ctype)),
    })
}

/// The image of a null value: blanks for characters, zero for numbers.
#[must_use]
pub fn null_image(ctype: i16, max: i32) -> Vec<u8> {
    let n = buffer_len(ctype, max);
    match ctype {
        ty::CHAR | ty::DATE | ty::TIME | ty::TIMESTAMP => vec![0x40; n],
        ty::DECIMAL | ty::NUMERIC => {
            let zero = Value::Num(Decimal::zero());
            dec_image(&zero, max, ctype == ty::NUMERIC).unwrap_or_else(|_| vec![0; n])
        }
        _ => vec![0; n.min(8).max(usize::from(matches!(ctype, ty::VARCHAR | ty::VARBINARY)) * 2)],
    }
}

/// The refusal of a C type this implementation does not take.
#[must_use]
pub fn unsupported(ctype: i16) -> Diag {
    Diag::cli("HY003", &format!("Buffers of C type {ctype} are not supported."))
}

/// A parameter's value from its buffer: `bytes` are the buffer (as many
/// as the binding says it has), `ind` the length word (`None` without
/// one), `plen` and `pscale` the binding's length or precision and scale.
pub fn value_of(ctype: i16, bytes: &[u8], ind: Option<i32>, plen: i32, pscale: i16) -> Result<Value, Diag> {
    if ind == Some(len::NULL_DATA) {
        return Ok(Value::Null);
    }
    let be = |n: usize| -> Result<&[u8], Diag> {
        bytes.get(..n).ok_or_else(|| Diag::cli("HY090", "A parameter's buffer is too short for its type."))
    };
    // A character length: the word, else up to the first NUL.
    let text_len = || -> usize {
        match ind {
            Some(n) if n >= 0 => usize::try_from(n).unwrap_or(0).min(bytes.len()),
            _ => bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len()),
        }
    };
    Ok(match ctype {
        ty::CHAR | ty::DATE | ty::TIME | ty::TIMESTAMP => Value::Str(ccsid::from_ebcdic(&bytes[..text_len()])),
        ty::VARCHAR | ty::VARBINARY => {
            let h = be(2)?;
            let n = usize::from(u16::from_be_bytes([h[0], h[1]])).min(bytes.len() - 2);
            let data = &bytes[2..2 + n];
            if ctype == ty::VARBINARY { Value::Bin(data.to_vec()) } else { Value::Str(ccsid::from_ebcdic(data)) }
        }
        ty::BINARY => {
            let n = match ind {
                Some(n) if n >= 0 => usize::try_from(n).unwrap_or(0).min(bytes.len()),
                _ => usize::try_from(plen).unwrap_or(bytes.len()).min(bytes.len()),
            };
            Value::Bin(bytes[..n].to_vec())
        }
        ty::INTEGER => {
            let b = be(4)?;
            Value::int(i64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])))
        }
        ty::SMALLINT => {
            let b = be(2)?;
            Value::int(i64::from(i16::from_be_bytes([b[0], b[1]])))
        }
        ty::BIGINT => {
            let b = be(8)?;
            Value::int(i64::from_be_bytes(b.try_into().expect("8 bytes")))
        }
        ty::DOUBLE | ty::FLOAT => Value::Double(f64::from_be_bytes(be(8)?.try_into().expect("8 bytes"))),
        ty::REAL => Value::Real(f32::from_be_bytes(be(4)?.try_into().expect("4 bytes"))),
        ty::DECIMAL | ty::NUMERIC => {
            let p = u8::try_from(plen).map_err(|_| out_of_range())?;
            let s = u8::try_from(pscale).map_err(|_| out_of_range())?;
            let d = if ctype == ty::DECIMAL {
                let c = Packed::new(p, s).map_err(|_| shape(0).unwrap_err())?;
                c.decode(be(c.byte_len())?)
            } else {
                let c = Zoned::new(p, s).map_err(|_| shape(0).unwrap_err())?;
                c.decode(be(c.byte_len())?)
            };
            Value::Num(d.map_err(|_| Diag { state: "22023".into(), native: -302, text: "A decimal parameter holds data that is not valid.".into() })?)
        }
        _ => return Err(unsupported(ctype)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(s: &str) -> Value {
        Value::Num(Decimal::parse(s).unwrap())
    }

    #[test]
    fn characters_end_with_a_nul_or_fill_the_buffer() {
        let i = image(&Value::Str("Ann".into()), ty::CHAR, 20, true).unwrap();
        assert_eq!(i.bytes, vec![0xC1, 0x95, 0x95, 0]);
        assert_eq!(i.ind, len::NTS);
        let i = image(&Value::Str("Ann".into()), ty::CHAR, 2, true).unwrap();
        assert_eq!(i.bytes, vec![0xC1, 0x95]);
        assert_eq!(i.ind, 2);
        let i = image(&Value::Str("2024-01-02".into()), ty::CHAR, 10, true).unwrap();
        assert_eq!((i.bytes.len(), i.ind), (10, 10));
    }

    #[test]
    fn decimals_truncate_their_scale() {
        let i = image(&num("99.99"), ty::DECIMAL, 5 * 256 + 1, true).unwrap();
        assert_eq!(i.bytes, vec![0x00, 0x99, 0x9C]);
        let i = image(&num("10.50"), ty::NUMERIC, 7 * 256 + 3, true).unwrap();
        assert_eq!(i.bytes, [0xF0, 0xF0, 0xF1, 0xF0, 0xF5, 0xF0, 0xC0]);
        assert!(image(&num("123456"), ty::DECIMAL, 5 * 256 + 1, true).is_err());
        assert_eq!(null_image(ty::DECIMAL, 9 * 256 + 2), vec![0, 0, 0, 0, 0x0C]);
        assert_eq!(null_image(ty::CHAR, 3), vec![0x40; 3]);
    }

    #[test]
    fn parameters_from_their_buffers() {
        assert_eq!(value_of(ty::INTEGER, &7i32.to_be_bytes(), None, 0, 0).unwrap(), Value::int(7));
        assert_eq!(value_of(ty::DECIMAL, &[0x01, 0x10, 0x0F], None, 5, 2).unwrap(), num("11.00"));
        assert_eq!(value_of(ty::CHAR, &[0xC1, 0xC2, 0, 0x40], Some(len::NTS), 4, 0).unwrap(), Value::Str("AB".into()));
        assert_eq!(value_of(ty::CHAR, &[0xC1, 0xC2, 0x40], Some(2), 3, 0).unwrap(), Value::Str("AB".into()));
        assert_eq!(value_of(ty::INTEGER, &[0; 4], Some(len::NULL_DATA), 0, 0).unwrap(), Value::Null);
        assert!(value_of(ty::WCHAR, &[0; 4], None, 0, 0).is_err());
    }
}
