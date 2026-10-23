// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Field values between the buffers and the display: a numeric field's
//! bytes become the text the operator sees (edited by `EDTCDE` or
//! `EDTWRD`, else the digits), and what the operator typed becomes the
//! field's bytes again.

use qdecimal::{Binary, Decimal, EditOpts, Float, Packed, Rounding, Shape, Zoned, edit_code, edit_word};

use crate::model::{Edit, Field, TypeClass};

/// Why typed text does not fit a field.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BadValue {
    /// Not a number.
    #[error("the value is not a number")]
    NotNumeric,
    /// More digits than the field holds.
    #[error("the value has more digits than the field holds")]
    TooLong,
    /// A date, time or timestamp of the wrong length.
    #[error("the value is not a valid date or time")]
    BadDateTime,
}

/// The number in a field's buffer bytes; `None` when the bytes do not
/// decode (a zeroed buffer, say).
#[must_use]
pub fn decode_number(f: &Field, bytes: &[u8]) -> Option<Decimal> {
    let digits = u8::try_from(f.length).ok()?;
    match f.ty.class {
        TypeClass::Zoned => Zoned::new(digits, f.decimals).ok()?.decode(bytes).ok(),
        TypeClass::Packed => Packed::new(digits, f.decimals).ok()?.decode(bytes).ok(),
        TypeClass::Binary => Binary::new(digits, f.decimals).ok()?.decode(bytes).ok(),
        TypeClass::Float => if bytes.len() <= 4 { Float::SINGLE } else { Float::DOUBLE }
            .decode(bytes)
            .ok(),
        _ => None,
    }
}

/// A number encoded for a field's buffer.
pub fn encode_number(f: &Field, v: &Decimal) -> Result<Vec<u8>, BadValue> {
    let digits = u8::try_from(f.length).map_err(|_| BadValue::TooLong)?;
    let r = match f.ty.class {
        TypeClass::Zoned => Zoned::new(digits, f.decimals)
            .and_then(|z| z.encode(v, Rounding::Truncate)),
        TypeClass::Packed => Packed::new(digits, f.decimals)
            .and_then(|p| p.encode(v, Rounding::Truncate)),
        TypeClass::Binary => Binary::new(digits, f.decimals)
            .and_then(|b| b.encode(v, Rounding::Truncate)),
        TypeClass::Float => if f.length <= 9 { Float::SINGLE } else { Float::DOUBLE }.encode(v),
        _ => return Err(BadValue::NotNumeric),
    };
    r.map_err(|_| BadValue::TooLong)
}

/// Zero in a field's buffer.
#[must_use]
pub fn zero_bytes(f: &Field) -> Vec<u8> {
    encode_number(f, &Decimal::zero()).unwrap_or_else(|_| vec![0; f.bytes as usize])
}

/// The digits of a number as DDS shows them without editing: the
/// field's digits, zero-filled, no point, the sign in the last digit's
/// zone (`J`-`R` and `}` for negatives).
#[must_use]
pub fn plain_digits(v: &Decimal, length: u32, decimals: u8) -> String {
    let digits = u8::try_from(length).unwrap_or(1).max(1);
    let shape = Shape::new(digits, decimals.min(digits)).unwrap_or(Shape {
        digits,
        scale: 0,
    });
    let fitted = v
        .fit_truncate(shape, Rounding::Truncate)
        .unwrap_or_else(|_| Decimal::zero());
    let mut s: String = fitted
        .char_text(shape.scale)
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    while s.len() < usize::from(digits) {
        s.insert(0, '0');
    }
    if s.len() > usize::from(digits) {
        s = s[s.len() - usize::from(digits)..].to_string();
    }
    if fitted.is_negative() && !fitted.is_zero()
        && let Some(last) = s.pop()
    {
        s.push(match last {
            '0' => '}',
            d => (b'J' + (d as u8 - b'1')) as char,
        });
    }
    s
}

/// A numeric field's text for the display, right-adjusted in its
/// screen length.
#[must_use]
pub fn display_number(f: &Field, v: &Decimal) -> String {
    let digits = u8::try_from(f.length).unwrap_or(1).max(1);
    let shape = Shape::new(digits, f.decimals.min(digits)).unwrap_or(Shape {
        digits,
        scale: 0,
    });
    let text = match &f.edit {
        Edit::Code(c, modifier) => {
            let opts = EditOpts {
                fill: match modifier.as_deref() {
                    Some("*") => qdecimal::Fill::Asterisk,
                    Some(sym) if sym.chars().count() == 1 => {
                        qdecimal::Fill::Currency(sym.chars().next().unwrap_or('$'))
                    }
                    _ => qdecimal::Fill::Blank,
                },
                ..EditOpts::default()
            };
            edit_code(v, shape, *c, &opts).unwrap_or_else(|_| plain_digits(v, f.length, f.decimals))
        }
        Edit::Word(w) => {
            edit_word(v, shape, w).unwrap_or_else(|_| plain_digits(v, f.length, f.decimals))
        }
        Edit::None => plain_digits(v, f.length, f.decimals),
    };
    let width = f.screen_len.max(1) as usize;
    let n = text.chars().count();
    if n >= width {
        text.chars().skip(n - width).collect()
    } else {
        format!("{text:>width$}")
    }
}

/// The number the operator typed: digits with an optional sign and
/// decimal point, blanks ignored; blank is zero.
pub fn parse_typed(f: &Field, text: &str) -> Result<Decimal, BadValue> {
    let mut s = String::new();
    let mut neg = false;
    for ch in text.chars() {
        match ch {
            ' ' | ',' | '+' => {}
            '-' => neg = true,
            '.' => s.push('.'),
            d if d.is_ascii_digit() => s.push(d),
            // A negative zone in the last digit, as DDS shows it.
            'J'..='R' => {
                neg = true;
                s.push((b'1' + (ch as u8 - b'J')) as char);
            }
            '}' => {
                neg = true;
                s.push('0');
            }
            _ => return Err(BadValue::NotNumeric),
        }
    }
    if s.is_empty() || s == "." {
        return Ok(Decimal::zero());
    }
    let mut v = Decimal::parse(&s).map_err(|_| BadValue::NotNumeric)?;
    if neg {
        v = v.negated();
    }
    let digits = u8::try_from(f.length).map_err(|_| BadValue::TooLong)?;
    let shape = Shape::new(digits.max(1), f.decimals.min(digits.max(1)))
        .map_err(|_| BadValue::TooLong)?;
    v.fit(shape, Rounding::Truncate).map_err(|_| BadValue::TooLong)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FieldType, Usage};

    fn zoned(len: u32, dec: u8) -> Field {
        let mut f = Field::new(Some("N".into()), FieldType::zoned(), len, dec, Usage::Both);
        f.bytes = len;
        f.screen_len = len + u32::from(dec > 0) + 1;
        f
    }

    #[test]
    fn digits_without_editing() {
        let v = Decimal::parse("-120").unwrap();
        assert_eq!(plain_digits(&v, 5, 0), "0012}");
        let v = Decimal::parse("-121").unwrap();
        assert_eq!(plain_digits(&v, 5, 0), "0012J");
        let v = Decimal::parse("12.5").unwrap();
        assert_eq!(plain_digits(&v, 5, 2), "01250");
    }

    #[test]
    fn edited_and_parsed() {
        let mut f = zoned(7, 2);
        f.edit = Edit::Code('J', None);
        let v = Decimal::parse("-1234.5").unwrap();
        let shown = display_number(&f, &v);
        assert!(shown.ends_with("1,234.50-"), "{shown:?}");
        let back = parse_typed(&f, " 1234.5-").unwrap();
        assert_eq!(back.char_text(2), "-1234.50");
        let bytes = encode_number(&f, &back).unwrap();
        assert_eq!(decode_number(&f, &bytes).unwrap().char_text(2), "-1234.50");
        assert!(parse_typed(&f, "   ").unwrap().is_zero());
        assert_eq!(parse_typed(&f, "12x"), Err(BadValue::NotNumeric));
        assert_eq!(parse_typed(&f, "1234567"), Err(BadValue::TooLong));
    }
}
