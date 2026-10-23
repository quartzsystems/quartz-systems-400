// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dialog variables in a program's buffer: each variable of a variable
//! record in turn, stored as its class's base type says (`CHAR n` and
//! the names as blank-padded EBCDIC, `BIN` big-endian, `PACKED` and
//! `ZONED` as such, `FLOAT` IEEE big-endian). The dialog keeps every
//! value as text.

use qdecimal::{Decimal, Packed, Rounding, Zoned};
use uim::PanelGroup;
use uim::model::BaseType;

/// How a variable is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Store {
    /// EBCDIC text of this many bytes.
    Char(usize),
    /// A big-endian integer of 2 or 4 bytes.
    Bin(usize),
    /// Packed decimal: digits, scale.
    Packed(u8, u8),
    /// Zoned decimal: digits, scale.
    Zoned(u8, u8),
    /// IEEE floating point of 4 or 8 bytes.
    Float(usize),
}

impl Store {
    /// Its length in the buffer.
    pub(super) fn len(self) -> usize {
        match self {
            Self::Char(n) | Self::Bin(n) | Self::Float(n) => n,
            Self::Packed(d, _) => usize::from(d) / 2 + 1,
            Self::Zoned(d, _) => usize::from(d),
        }
    }
}

/// How a variable of the group is stored, `None` when it is not one.
pub(super) fn store_of(group: &PanelGroup, var: &str) -> Option<Store> {
    let v = group.variable(var)?;
    let class = group.class(&v.class)?;
    let size = |dft: u32| v.size.or(class.width).unwrap_or(dft) as usize;
    let digits = |d: u32| u8::try_from(d.clamp(1, 63)).unwrap_or(15);
    Some(match class.basetype {
        BaseType::Char(n) | BaseType::Igc(n) => Store::Char(v.size.unwrap_or(n) as usize),
        BaseType::Bin(15 | 16) => Store::Bin(2),
        BaseType::Bin(_) => Store::Bin(4),
        BaseType::Packed(d, p) => Store::Packed(digits(d), digits(p).min(digits(d))),
        BaseType::Zoned(d, p) => Store::Zoned(digits(d), digits(p).min(digits(d))),
        BaseType::Float(4) => Store::Float(4),
        BaseType::Float(_) => Store::Float(8),
        BaseType::Date | BaseType::ObjName | BaseType::Name | BaseType::SName => Store::Char(10),
        BaseType::Time => Store::Char(8),
        BaseType::ObjNameQ => Store::Char(20),
        BaseType::Ptr => Store::Char(16),
        BaseType::Action => Store::Char(size(2)),
        BaseType::Dec => Store::Char(size(15)),
        BaseType::CmdLine => Store::Char(size(153)),
    })
}

/// A stored value as the dialog's text; `None` when the bytes are not
/// a value of the type.
pub(super) fn decode(st: Store, b: &[u8]) -> Option<String> {
    Some(match st {
        Store::Char(_) => ccsid::from_ebcdic(b).trim_end().to_string(),
        Store::Bin(2) => i16::from_be_bytes(b.get(..2)?.try_into().ok()?).to_string(),
        Store::Bin(_) => i32::from_be_bytes(b.get(..4)?.try_into().ok()?).to_string(),
        Store::Packed(d, p) => Packed::new(d, p).ok()?.decode(b).ok()?.to_string(),
        Store::Zoned(d, p) => Zoned::new(d, p).ok()?.decode(b).ok()?.to_string(),
        Store::Float(4) => f32::from_be_bytes(b.get(..4)?.try_into().ok()?).to_string(),
        Store::Float(_) => f64::from_be_bytes(b.get(..8)?.try_into().ok()?).to_string(),
    })
}

/// The dialog's text as stored; `None` when it is not a value of the
/// type (a blank number is zero).
pub(super) fn encode(st: Store, value: &str) -> Option<Vec<u8>> {
    let t = value.trim();
    let num = if t.is_empty() { "0" } else { t };
    Some(match st {
        Store::Char(n) => {
            let mut b = ccsid::to_ebcdic(value.trim_end()).unwrap_or_default();
            b.resize(n, 0x40);
            b
        }
        Store::Bin(2) => num.parse::<i16>().ok()?.to_be_bytes().to_vec(),
        Store::Bin(_) => num.parse::<i32>().ok()?.to_be_bytes().to_vec(),
        Store::Packed(d, p) => {
            let v = Decimal::parse(num).ok()?;
            Packed::new(d, p)
                .ok()?
                .encode(&v, Rounding::HalfAdjust)
                .ok()?
        }
        Store::Zoned(d, p) => {
            let v = Decimal::parse(num).ok()?;
            Zoned::new(d, p)
                .ok()?
                .encode(&v, Rounding::HalfAdjust)
                .ok()?
        }
        Store::Float(4) => num.parse::<f32>().ok()?.to_be_bytes().to_vec(),
        Store::Float(_) => num.parse::<f64>().ok()?.to_be_bytes().to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for (st, v) in [
            (Store::Char(6), "ABC"),
            (Store::Bin(2), "-12"),
            (Store::Bin(4), "70000"),
            (Store::Packed(7, 2), "123.45"),
            (Store::Zoned(5, 0), "42"),
        ] {
            let b = encode(st, v).unwrap();
            assert_eq!(b.len(), st.len());
            assert_eq!(decode(st, &b).unwrap(), v);
        }
        assert!(encode(Store::Bin(4), "x").is_none());
        assert_eq!(
            decode(
                Store::Packed(3, 0),
                &encode(Store::Packed(3, 0), "").unwrap()
            )
            .unwrap(),
            "0"
        );
    }
}
