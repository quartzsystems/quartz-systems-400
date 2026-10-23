// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A record format's fields and their values: bytes of the record
//! shown as text in a DFU input field, and text typed back into bytes.

use qdecimal::{Binary, Decimal, Float, Packed, Rounding, Zoned};

use crate::DfuError;

/// A field of a record format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    /// Name.
    pub name: String,
    /// DDS data type: `A`, `P`, `S`, `B`, `F`, `L`, `T`, `Z`, ...
    pub ty: char,
    /// Buffer position, from 1.
    pub pos: u32,
    /// Bytes.
    pub len: u32,
    /// Digits of a numeric field.
    pub digits: u32,
    /// Decimal positions.
    pub decimals: u32,
    /// Part of the key.
    pub key: bool,
}

impl Field {
    /// Numeric: packed, zoned, binary or float.
    #[must_use]
    pub fn numeric(&self) -> bool {
        matches!(self.ty, 'P' | 'S' | 'B' | 'F')
    }

    /// Positions the field takes on the display: a numeric field's
    /// digits with room for the point and the sign, a character
    /// field's bytes.
    #[must_use]
    pub fn width(&self) -> usize {
        if self.numeric() {
            let d = self.digits.max(1) as usize;
            d + usize::from(self.decimals > 0) + 1
        } else {
            self.len as usize
        }
    }

    fn bytes<'a>(&self, rec: &'a [u8]) -> &'a [u8] {
        let s = (self.pos.max(1) - 1) as usize;
        let e = (s + self.len as usize).min(rec.len());
        rec.get(s.min(e)..e).unwrap_or(&[])
    }

    fn shape(&self) -> (u8, u8) {
        (
            u8::try_from(self.digits.clamp(1, 63)).unwrap_or(63),
            u8::try_from(self.decimals.min(63)).unwrap_or(0),
        )
    }

    /// The value in a record, as the display shows it: characters
    /// trimmed on the right, numbers with every decimal and no editing.
    #[must_use]
    pub fn decode(&self, rec: &[u8]) -> String {
        let b = self.bytes(rec);
        if !self.numeric() {
            return ccsid::from_ebcdic(b).trim_end().to_string();
        }
        let (d, s) = self.shape();
        let v = match self.ty {
            'P' => Packed::new(d, s).and_then(|p| p.decode(b)),
            'S' => Zoned::new(d, s).and_then(|z| z.decode(b)),
            'B' => Binary::new(d.min(18), s).and_then(|x| x.decode(b)),
            _ => if b.len() == 8 { Float::DOUBLE } else { Float::SINGLE }.decode(b),
        };
        v.map_or_else(|_| "*".repeat(self.width().min(9)), |v| text(&v, s))
    }

    /// The value as the field's input shows it: numbers right-adjusted
    /// in their width, the last position for the sign.
    #[must_use]
    pub fn shown(&self, rec: &[u8]) -> String {
        let v = self.decode(rec);
        if self.numeric() {
            signed(&v, self.width())
        } else {
            v
        }
    }

    /// Typed text into the record's bytes.
    ///
    /// # Errors
    /// Text that is not a number for a numeric field, a number too large
    /// for it, or a character the code page lacks.
    pub fn encode(&self, typed: &str, rec: &mut [u8]) -> Result<(), DfuError> {
        let start = (self.pos.max(1) - 1) as usize;
        let end = start + self.len as usize;
        if end > rec.len() {
            return Err(DfuError::Layout(self.name.clone()));
        }
        let out = &mut rec[start..end];
        if !self.numeric() {
            return ccsid::encode_fixed(typed.trim_end(), out)
                .map_err(|_| DfuError::BadData(self.name.clone()));
        }
        let t = typed.trim();
        let v = if t.is_empty() {
            Decimal::zero()
        } else {
            parse_number(t).ok_or_else(|| DfuError::NotNumeric(self.name.clone()))?
        };
        let (d, sc) = self.shape();
        let big = |_| DfuError::TooLarge(self.name.clone());
        let bytes = match self.ty {
            'P' => Packed::new(d, sc).and_then(|p| p.encode(&v, Rounding::HalfAdjust)),
            'S' => Zoned::new(d, sc).and_then(|z| z.encode(&v, Rounding::HalfAdjust)),
            'B' => Binary::new(d.min(18), sc).and_then(|x| x.encode(&v, Rounding::HalfAdjust)),
            _ => if out.len() == 8 { Float::DOUBLE } else { Float::SINGLE }.encode(&v),
        }
        .map_err(big)?;
        if bytes.len() != out.len() {
            return Err(DfuError::Layout(self.name.clone()));
        }
        out.copy_from_slice(&bytes);
        Ok(())
    }
}

/// `-12.5`, `12.5-`, `+3`, `.5`: a number as an operator types it.
fn parse_number(t: &str) -> Option<Decimal> {
    let (neg, body) = if let Some(b) = t.strip_suffix('-') {
        (true, b.trim())
    } else if let Some(b) = t.strip_prefix('-') {
        (true, b.trim())
    } else {
        (false, t.strip_prefix('+').unwrap_or(t).trim())
    };
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    let v = Decimal::parse(body).ok()?;
    Some(if neg { v.negated() } else { v })
}

/// A number right-adjusted in `width`, its sign (blank or `-`) in the
/// last position.
#[must_use]
pub fn signed(v: &str, width: usize) -> String {
    let (neg, digits) = match v.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, v),
    };
    format!(
        "{digits:>w$}{}",
        if neg { '-' } else { ' ' },
        w = width.saturating_sub(1)
    )
}

/// A value with exactly `scale` decimals and a leading minus.
fn text(v: &Decimal, scale: u8) -> String {
    let v = v.rescale(scale, Rounding::Truncate).unwrap_or_else(|_| v.clone());
    v.to_string()
}

/// A blank record of a format's length: blanks in character fields,
/// zero in numeric ones.
#[must_use]
pub fn blank_record(fields: &[Field], len: usize) -> Vec<u8> {
    let mut rec = vec![0x40; len];
    for f in fields.iter().filter(|f| f.numeric()) {
        let _ = f.encode("0", &mut rec);
    }
    rec
}

/// The key of a record: the key fields' bytes in record layout, in key
/// order (the order of `keys`, by name).
#[must_use]
pub fn key_bytes(fields: &[Field], keys: &[String], rec: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for k in keys {
        if let Some(f) = fields.iter().find(|f| f.name == *k) {
            out.extend_from_slice(f.bytes(rec));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, ty: char, pos: u32, len: u32, digits: u32, decimals: u32) -> Field {
        Field {
            name: name.into(),
            ty,
            pos,
            len,
            digits,
            decimals,
            key: false,
        }
    }

    #[test]
    fn round_trips() {
        let fields = [
            f("ID", 'S', 1, 9, 9, 0),
            f("NAME", 'A', 10, 5, 0, 0),
            f("AMT", 'P', 15, 4, 7, 2),
            f("CNT", 'B', 19, 4, 9, 0),
        ];
        let mut rec = blank_record(&fields, 22);
        assert_eq!(fields[0].decode(&rec), "0");
        fields[0].encode("1", &mut rec).unwrap();
        fields[1].encode("abc", &mut rec).unwrap();
        fields[2].encode("-12.5", &mut rec).unwrap();
        fields[3].encode("42", &mut rec).unwrap();
        assert_eq!(fields[0].shown(&rec), "        1 ");
        assert_eq!(fields[1].decode(&rec), "abc");
        assert_eq!(fields[2].decode(&rec), "-12.50");
        assert_eq!(fields[3].decode(&rec), "42");
        fields[2].encode("3.25-", &mut rec).unwrap();
        assert_eq!(fields[2].decode(&rec), "-3.25");
    }

    #[test]
    fn refuses() {
        let fld = f("ID", 'S', 1, 3, 3, 0);
        let mut rec = vec![0xF0; 3];
        assert_eq!(
            fld.encode("1x", &mut rec),
            Err(DfuError::NotNumeric("ID".into()))
        );
        assert_eq!(
            fld.encode("12345", &mut rec),
            Err(DfuError::TooLarge("ID".into()))
        );
    }

    #[test]
    fn keys() {
        let mut fields = [f("A", 'A', 1, 2, 0, 0), f("B", 'A', 3, 2, 0, 0)];
        fields[1].key = true;
        let rec = ccsid::to_ebcdic("xxyy").unwrap();
        assert_eq!(
            key_bytes(&fields, &["B".into(), "A".into()], &rec),
            ccsid::to_ebcdic("yyxx").unwrap()
        );
    }
}
