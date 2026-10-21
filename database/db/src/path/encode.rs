// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key part encoding. See `docs/DB_FORMAT.md` §6.

use dds::{DataType, DateFmt, FieldDef, KeyField, KeyTransform, TimeFmt};
use qdecimal::Decimal;

use crate::error::{DbError, Result};
use crate::format::keys::part_len;
use crate::record::Value;

fn mapping(f: &FieldDef, what: impl std::fmt::Display) -> DbError {
    DbError::DataMapping {
        field: f.name.to_string(),
        what: what.to_string(),
    }
}

/// Append the key part of `value` for field `f` under key spec `k`.
pub fn key_part(f: &FieldDef, k: &KeyField, value: &Value, out: &mut Vec<u8>) -> Result<()> {
    let start = out.len();
    let data_len = part_len(f, k.transform) - usize::from(f.allow_null);
    if f.allow_null {
        out.push(if *value == Value::Null { 0xFF } else { 0x00 });
    }
    match value {
        Value::Null => out.extend(std::iter::repeat_n(0u8, data_len)),
        Value::Char(bytes) if f.data_type.is_character() => {
            let mut part: Vec<u8> = bytes.iter().take(data_len).copied().collect();
            part.resize(data_len, ccsid::SPACE);
            match k.transform {
                KeyTransform::Zone => part.iter_mut().for_each(|b| *b &= 0xF0),
                KeyTransform::Digit => part.iter_mut().for_each(|b| *b &= 0x0F),
                _ => {}
            }
            out.extend(part);
        }
        Value::Char(bytes) => out.extend(datetime(f, bytes)?),
        Value::Num(d) if f.data_type == DataType::Float => {
            let x: f64 = d.to_string().parse().map_err(|_| mapping(f, "float"))?;
            let bits = x.to_bits();
            let ordered = if x.is_sign_negative() {
                !bits
            } else {
                bits ^ (1 << 63)
            };
            out.extend(ordered.to_be_bytes());
        }
        Value::Num(d) => out.extend(decimal(f, k.transform, d)),
    }
    if k.descend {
        for b in &mut out[start..] {
            *b = !*b;
        }
    }
    debug_assert_eq!(out.len() - start, part_len(f, k.transform));
    Ok(())
}

/// Sign byte and digit bytes of a decimal key part.
fn decimal(f: &FieldDef, transform: KeyTransform, d: &Decimal) -> Vec<u8> {
    let neg = d.is_negative() && transform == KeyTransform::Signed;
    let digits = usize::try_from(f.length).expect("fits");
    let mut out = Vec::with_capacity(1 + digits);
    out.push(if neg { 0x01 } else { 0x02 });
    let scaled = d
        .rescale(f.decimals, qdecimal::Rounding::Truncate)
        .unwrap_or_else(|_| Decimal::zero());
    let mag = scaled.digits_le();
    for i in (0..digits).rev() {
        let digit = mag.get(i).copied().unwrap_or(0);
        out.push(if neg { 0x39 - digit } else { 0x30 + digit });
    }
    out
}

fn ascii_digits(text: &str, f: &FieldDef) -> Result<Vec<u32>> {
    text.chars()
        .filter(char::is_ascii_digit)
        .map(|c| c.to_digit(10).ok_or_else(|| mapping(f, "digit")))
        .collect()
}

/// `yyyymmdd`, `hhmmss` or the timestamp bytes.
fn datetime(f: &FieldDef, bytes: &[u8]) -> Result<Vec<u8>> {
    let text = ccsid::from_ebcdic(bytes);
    let digits = ascii_digits(&text, f)?;
    let num = |ds: &[u32]| ds.iter().fold(0u32, |acc, d| acc * 10 + d);
    let out: Vec<u32> = match f.data_type {
        DataType::Date => {
            let fmt = f.date_fmt.unwrap_or_default();
            let (y, m, d) = match fmt {
                DateFmt::Iso | DateFmt::Jis if digits.len() == 8 => {
                    (num(&digits[..4]), num(&digits[4..6]), num(&digits[6..8]))
                }
                DateFmt::Usa if digits.len() == 8 => {
                    (num(&digits[4..8]), num(&digits[..2]), num(&digits[2..4]))
                }
                DateFmt::Eur if digits.len() == 8 => {
                    (num(&digits[4..8]), num(&digits[2..4]), num(&digits[..2]))
                }
                DateFmt::Mdy if digits.len() == 6 => (
                    century(num(&digits[4..6])),
                    num(&digits[..2]),
                    num(&digits[2..4]),
                ),
                DateFmt::Dmy if digits.len() == 6 => (
                    century(num(&digits[4..6])),
                    num(&digits[2..4]),
                    num(&digits[..2]),
                ),
                DateFmt::Ymd if digits.len() == 6 => (
                    century(num(&digits[..2])),
                    num(&digits[2..4]),
                    num(&digits[4..6]),
                ),
                DateFmt::Jul if digits.len() == 5 => {
                    (century(num(&digits[..2])), 0, num(&digits[2..5]))
                }
                _ => return Err(mapping(f, format!("date {text:?} in {}", fmt.name()))),
            };
            let md = if fmt == DateFmt::Jul { d } else { m * 100 + d };
            vec![
                y / 1000,
                y / 100 % 10,
                y / 10 % 10,
                y % 10,
                md / 1000,
                md / 100 % 10,
                md / 10 % 10,
                md % 10,
            ]
        }
        DataType::Time => {
            let fmt = f.time_fmt.unwrap_or_default();
            let (mut h, m, s) = if fmt == TimeFmt::Usa && digits.len() == 4 {
                (num(&digits[..2]), num(&digits[2..4]), 0)
            } else if digits.len() == 6 {
                (num(&digits[..2]), num(&digits[2..4]), num(&digits[4..6]))
            } else {
                return Err(mapping(f, format!("time {text:?} in {}", fmt.name())));
            };
            if fmt == TimeFmt::Usa {
                let pm = text.to_ascii_uppercase().contains("PM");
                if h == 12 {
                    h = 0;
                }
                if pm {
                    h += 12;
                }
            }
            vec![h / 10, h % 10, m / 10, m % 10, s / 10, s % 10]
        }
        _ => return Ok(bytes.to_vec()),
    };
    Ok(out
        .into_iter()
        .map(|d| 0x30 + u8::try_from(d).expect("digit"))
        .collect())
}

/// Two-digit years: 40 to 99 are 19xx, 00 to 39 are 20xx.
fn century(yy: u32) -> u32 {
    if yy >= 40 { 1900 + yy } else { 2000 + yy }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsname::Name;

    fn fld(t: DataType, len: u32, dec: u8) -> FieldDef {
        FieldDef::new(Name::parse("K").unwrap(), t, len, dec)
    }

    fn key(f: &FieldDef, transform: KeyTransform, descend: bool, v: &Value) -> Vec<u8> {
        let k = KeyField {
            name: f.name,
            field: 0,
            descend,
            transform,
            noaltseq: false,
            line: 0,
        };
        let mut out = Vec::new();
        key_part(f, &k, v, &mut out).unwrap();
        out
    }

    fn n(s: &str) -> Value {
        Value::Num(Decimal::parse(s).unwrap())
    }

    #[test]
    fn numbers_order_bytewise() {
        let f = fld(DataType::Packed, 5, 2);
        let vals = ["-999.99", "-1", "-0.01", "0", "0.01", "1", "999.99"];
        let keys: Vec<Vec<u8>> = vals
            .iter()
            .map(|v| key(&f, KeyTransform::Signed, false, &n(v)))
            .collect();
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "{:?} < {:?}", w[0], w[1]);
        }
        assert_eq!(keys[3], vec![0x02, 0x30, 0x30, 0x30, 0x30, 0x30]);
        assert_eq!(
            key(&f, KeyTransform::Absval, false, &n("-1")),
            key(&f, KeyTransform::Signed, false, &n("1"))
        );
        let desc: Vec<Vec<u8>> = vals
            .iter()
            .map(|v| key(&f, KeyTransform::Signed, true, &n(v)))
            .collect();
        for w in desc.windows(2) {
            assert!(w[0] > w[1]);
        }
        let fl = fld(DataType::Float, 9, 2);
        let a = key(&fl, KeyTransform::Signed, false, &n("-2.5"));
        let b = key(&fl, KeyTransform::Signed, false, &n("0"));
        let c = key(&fl, KeyTransform::Signed, false, &n("3.25"));
        assert!(a < b && b < c);
    }

    #[test]
    fn text_zone_digit_and_null() {
        let mut f = fld(DataType::Char, 4, 0);
        let v = Value::Char(vec![0xC1, 0xF2]);
        assert_eq!(
            key(&f, KeyTransform::Unsigned, false, &v),
            vec![0xC1, 0xF2, 0x40, 0x40]
        );
        assert_eq!(
            key(&f, KeyTransform::Zone, false, &v),
            vec![0xC0, 0xF0, 0x40, 0x40]
        );
        assert_eq!(
            key(&f, KeyTransform::Digit, false, &v),
            vec![0x01, 0x02, 0x00, 0x00]
        );
        f.allow_null = true;
        assert_eq!(key(&f, KeyTransform::Unsigned, false, &v)[0], 0x00);
        assert_eq!(
            key(&f, KeyTransform::Unsigned, false, &Value::Null),
            vec![0xFF, 0, 0, 0, 0]
        );
        assert!(
            key(&f, KeyTransform::Unsigned, false, &Value::Null)
                > key(&f, KeyTransform::Unsigned, false, &v)
        );
    }

    #[test]
    fn dates_and_times() {
        let mut d = fld(DataType::Date, 8, 0);
        d.date_fmt = Some(DateFmt::Mdy);
        let v = Value::Char(ccsid::to_ebcdic("12/31/99").unwrap());
        assert_eq!(
            key(&d, KeyTransform::Unsigned, false, &v),
            b"19991231".to_vec()
        );
        let v = Value::Char(ccsid::to_ebcdic("01/02/26").unwrap());
        assert_eq!(
            key(&d, KeyTransform::Unsigned, false, &v),
            b"20260102".to_vec()
        );
        d.date_fmt = Some(DateFmt::Usa);
        let v = Value::Char(ccsid::to_ebcdic("10/10/2026").unwrap());
        assert_eq!(
            key(&d, KeyTransform::Unsigned, false, &v),
            b"20261010".to_vec()
        );
        d.date_fmt = Some(DateFmt::Jul);
        let v = Value::Char(ccsid::to_ebcdic("26/283").unwrap());
        assert_eq!(
            key(&d, KeyTransform::Unsigned, false, &v),
            b"20260283".to_vec()
        );
        let mut t = fld(DataType::Time, 8, 0);
        t.time_fmt = Some(TimeFmt::Usa);
        let v = Value::Char(ccsid::to_ebcdic("01:30 PM").unwrap());
        assert_eq!(
            key(&t, KeyTransform::Unsigned, false, &v),
            b"133000".to_vec()
        );
        t.time_fmt = Some(TimeFmt::Iso);
        let v = Value::Char(ccsid::to_ebcdic("23.59.58").unwrap());
        assert_eq!(
            key(&t, KeyTransform::Unsigned, false, &v),
            b"235958".to_vec()
        );
        let bad = Value::Char(ccsid::to_ebcdic("nonsense").unwrap());
        let k = KeyField {
            name: t.name,
            field: 0,
            descend: false,
            transform: KeyTransform::Unsigned,
            noaltseq: false,
            line: 0,
        };
        assert!(key_part(&t, &k, &bad, &mut Vec::new()).is_err());
    }
}
