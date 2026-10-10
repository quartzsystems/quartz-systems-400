// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DTAARA`: `*CHAR`, `*DEC` and `*LGL` data areas. See the format
//! spec §10.

use std::fmt;

use crate::obj::ObjectId;
use crate::obj::ops::{self, CreateParams};
use crate::obj::space::Space;
use crate::obj::types;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes of the attribute area used by the fixed part.
pub const ATTR_LEN: usize = 16;
/// Body offset of an inline value, relative to the attribute area.
pub const INLINE_OFF: usize = 16;
/// Longest value kept in the attribute area; longer ones get a page.
pub const INLINE_MAX: usize = crate::obj::header::ATTR_LEN - INLINE_OFF;
/// Longest `*CHAR` value.
pub const MAX_CHAR: u16 = 2000;
/// Most `*DEC` digits.
pub const MAX_DIGITS: u8 = 24;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "DtaaraAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("kind", 0, 1),
        field("decimals", 1, 1),
        field("length", 2, 2),
        field("locked", 4, 1),
        field("inline", 5, 1),
        field("reserved", 6, 2),
        field("reserved2", 8, 8),
    ],
};

/// The kind and size of a data area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataAreaType {
    /// `*CHAR` of `len` bytes, 1..=2000.
    Char {
        /// Length.
        len: u16,
    },
    /// `*DEC` with `digits` total and `decimals` after the point.
    Dec {
        /// Total digits, 1..=24.
        digits: u8,
        /// Decimal positions, 0..=9, at most `digits`.
        decimals: u8,
    },
    /// `*LGL`.
    Lgl,
}

impl DataAreaType {
    fn check(self) -> Result<()> {
        let ok = match self {
            Self::Char { len } => (1..=MAX_CHAR).contains(&len),
            Self::Dec { digits, decimals } => {
                (1..=MAX_DIGITS).contains(&digits) && decimals <= 9 && decimals <= digits
            }
            Self::Lgl => true,
        };
        if ok {
            Ok(())
        } else {
            Err(StoreError::BadValue {
                what: format!("data area type {self:?}"),
            })
        }
    }

    /// Whether the value lives in the attribute area.
    #[must_use]
    pub fn inline(self) -> bool {
        self.value_len() <= INLINE_MAX
    }

    /// Bytes the value occupies.
    #[must_use]
    pub fn value_len(self) -> usize {
        match self {
            Self::Char { len } => usize::from(len),
            Self::Dec { .. } => 16,
            Self::Lgl => 1,
        }
    }

    fn encode(self, locked: bool) -> [u8; ATTR_LEN] {
        let mut a = [0u8; ATTR_LEN];
        match self {
            Self::Char { len } => {
                a[0] = 0;
                a[2..4].copy_from_slice(&len.to_le_bytes());
            }
            Self::Dec { digits, decimals } => {
                a[0] = 1;
                a[1] = decimals;
                a[2..4].copy_from_slice(&u16::from(digits).to_le_bytes());
            }
            Self::Lgl => {
                a[0] = 2;
                a[2..4].copy_from_slice(&1u16.to_le_bytes());
            }
        }
        a[4] = u8::from(locked);
        a[5] = u8::from(self.inline());
        a
    }

    fn decode(a: &[u8]) -> Result<(Self, bool)> {
        let length = u16::from_le_bytes([a[2], a[3]]);
        let t = match a[0] {
            0 => Self::Char { len: length },
            1 => Self::Dec {
                digits: u8::try_from(length).unwrap_or(255),
                decimals: a[1],
            },
            2 => Self::Lgl,
            k => {
                return Err(StoreError::BadValue {
                    what: format!("data area kind {k}"),
                });
            }
        };
        t.check()?;
        Ok((t, a[4] != 0))
    }
}

/// A data area's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataAreaValue {
    /// Text, at most the area's length; stored space padded.
    Char(String),
    /// A scaled integer: the value times `10^decimals`.
    Dec(i128),
    /// `'1'` or `'0'`.
    Lgl(bool),
}

/// A decimal with its scale, for display and parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decimal {
    /// Value times `10^decimals`.
    pub scaled: i128,
    /// Decimal positions.
    pub decimals: u8,
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.scaled < 0 { "-" } else { "" };
        let mag = self.scaled.unsigned_abs();
        if self.decimals == 0 {
            return write!(f, "{sign}{mag}");
        }
        let scale = 10u128.pow(u32::from(self.decimals));
        write!(
            f,
            "{sign}{}.{:0width$}",
            mag / scale,
            mag % scale,
            width = usize::from(self.decimals)
        )
    }
}

impl Decimal {
    /// Parse `[-]digits[.digits]` with at most `decimals` fraction
    /// digits and `digits` significant digits in all.
    pub fn parse(s: &str, digits: u8, decimals: u8) -> Result<Self> {
        let bad = || StoreError::BadValue {
            what: format!("{s:?} is not a {digits},{decimals} decimal"),
        };
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int, frac) = body.split_once('.').unwrap_or((body, ""));
        if (int.is_empty() && frac.is_empty())
            || !int.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
            || frac.len() > usize::from(decimals)
        {
            return Err(bad());
        }
        let int_digits = int.trim_start_matches('0').len();
        if int_digits > usize::from(digits - decimals) {
            return Err(bad());
        }
        let mut text = String::from(int);
        text.push_str(frac);
        for _ in frac.len()..usize::from(decimals) {
            text.push('0');
        }
        let mag: i128 = if text.is_empty() {
            0
        } else {
            text.parse().map_err(|_| bad())?
        };
        Ok(Self {
            scaled: if neg { -mag } else { mag },
            decimals,
        })
    }
}

/// Create a data area with an initial value.
pub fn create(
    txn: &mut Txn<'_>,
    params: &CreateParams,
    kind: DataAreaType,
    initial: &DataAreaValue,
) -> Result<ObjectId> {
    kind.check()?;
    let params = CreateParams {
        type_code: types::DTAARA,
        ..params.clone()
    };
    let id = ops::create(txn, &params)?;
    let page = ops::read_header(txn, id)?.0;
    ops::write_attr(txn, page, 0, &kind.encode(false))?;
    if !kind.inline() {
        Space::ensure(txn, id, kind.value_len() as u64, 0x40)?;
    }
    write(txn, id, initial)?;
    Ok(id)
}

/// The type and lock flag.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<(DataAreaType, bool)> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::DTAARA {
        return Err(StoreError::ObjectNotFound {
            name: h.name.to_string(),
            library: String::new(),
        });
    }
    let a = ops::read_attr(txn, page)?;
    DataAreaType::decode(&a[..ATTR_LEN])
}

/// The value.
pub fn read(txn: &mut Txn<'_>, id: ObjectId) -> Result<DataAreaValue> {
    let (kind, _) = describe(txn, id)?;
    let raw = read_value(txn, id, kind, 0, kind.value_len())?;
    Ok(match kind {
        DataAreaType::Char { .. } => DataAreaValue::Char(ccsid::from_ebcdic(&raw)),
        DataAreaType::Dec { .. } => {
            DataAreaValue::Dec(i128::from_le_bytes(raw[..16].try_into().expect("16")))
        }
        DataAreaType::Lgl => DataAreaValue::Lgl(raw[0] != 0),
    })
}

fn check_dec(kind: DataAreaType, scaled: i128) -> Result<()> {
    let DataAreaType::Dec { digits, .. } = kind else {
        return Err(StoreError::BadValue {
            what: "not a *DEC data area".into(),
        });
    };
    if scaled.unsigned_abs() >= 10u128.pow(u32::from(digits)) {
        return Err(StoreError::BadValue {
            what: format!("{scaled} exceeds {digits} digits"),
        });
    }
    Ok(())
}

/// Replace the whole value.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, value: &DataAreaValue) -> Result<()> {
    let (kind, _) = describe(txn, id)?;
    let bytes = match (kind, value) {
        (DataAreaType::Char { len }, DataAreaValue::Char(s)) => {
            let mut out = vec![0x40u8; usize::from(len)];
            ccsid::encode_fixed(s, &mut out).map_err(|e| StoreError::BadValue {
                what: e.to_string(),
            })?;
            out
        }
        (DataAreaType::Dec { .. }, DataAreaValue::Dec(v)) => {
            check_dec(kind, *v)?;
            v.to_le_bytes().to_vec()
        }
        (DataAreaType::Lgl, DataAreaValue::Lgl(b)) => vec![u8::from(*b)],
        _ => {
            return Err(StoreError::BadValue {
                what: format!("value {value:?} does not match {kind:?}"),
            });
        }
    };
    write_value(txn, id, kind, 0, &bytes)
}

/// Replace `len` characters from `start` (1-based) of a `*CHAR` value.
pub fn write_substring(
    txn: &mut Txn<'_>,
    id: ObjectId,
    start: u16,
    len: u16,
    s: &str,
) -> Result<()> {
    let (kind, _) = describe(txn, id)?;
    let DataAreaType::Char { len: total } = kind else {
        return Err(StoreError::BadValue {
            what: "substring on a non-*CHAR data area".into(),
        });
    };
    if start == 0 || len == 0 || u32::from(start) + u32::from(len) - 1 > u32::from(total) {
        return Err(StoreError::BadValue {
            what: format!("substring {start},{len} outside 1..{total}"),
        });
    }
    let mut out = vec![0x40u8; usize::from(len)];
    ccsid::encode_fixed(s, &mut out).map_err(|e| StoreError::BadValue {
        what: e.to_string(),
    })?;
    write_value(txn, id, kind, usize::from(start) - 1, &out)
}

/// `len` bytes of the value from `off`: the attribute area or the page.
fn read_value(
    txn: &mut Txn<'_>,
    id: ObjectId,
    kind: DataAreaType,
    off: usize,
    len: usize,
) -> Result<Vec<u8>> {
    if kind.inline() {
        let (page, _) = ops::read_header(txn, id)?;
        let attr = ops::read_attr(txn, page)?;
        return Ok(attr[INLINE_OFF + off..INLINE_OFF + off + len].to_vec());
    }
    let (space, _) = Space::load(txn, id)?;
    space.read(txn, off as u64, len)
}

/// Change `bytes` of the value at `off`, in place either way.
fn write_value(
    txn: &mut Txn<'_>,
    id: ObjectId,
    kind: DataAreaType,
    off: usize,
    bytes: &[u8],
) -> Result<()> {
    if kind.inline() {
        let (page, _) = ops::read_header(txn, id)?;
        return ops::write_attr(txn, page, INLINE_OFF + off, bytes);
    }
    let (space, _) = Space::load(txn, id)?;
    space.write(txn, off as u64, bytes)
}

/// Set the lock flag (stored only in Phase 2).
pub fn set_locked(txn: &mut Txn<'_>, id: ObjectId, locked: bool) -> Result<()> {
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 4, &[u8::from(locked)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals() {
        let d = Decimal::parse("-12.5", 5, 2).unwrap();
        assert_eq!(d.scaled, -1250);
        assert_eq!(d.to_string(), "-12.50");
        assert_eq!(Decimal::parse("7", 3, 0).unwrap().to_string(), "7");
        assert_eq!(Decimal::parse(".5", 3, 1).unwrap().scaled, 5);
        assert!(
            Decimal::parse("1234", 5, 2).is_err(),
            "too many integer digits"
        );
        assert!(Decimal::parse("1.234", 5, 2).is_err(), "too many decimals");
        assert!(Decimal::parse("x", 5, 2).is_err());
        assert_eq!(
            Decimal {
                scaled: 0,
                decimals: 3
            }
            .to_string(),
            "0.000"
        );
    }

    #[test]
    fn attr_round_trip() {
        for (t, locked) in [
            (DataAreaType::Char { len: 2000 }, false),
            (
                DataAreaType::Dec {
                    digits: 15,
                    decimals: 5,
                },
                true,
            ),
            (DataAreaType::Lgl, false),
        ] {
            assert_eq!(
                DataAreaType::decode(&t.encode(locked)).unwrap(),
                (t, locked)
            );
        }
        assert!(DataAreaType::Char { len: 2001 }.check().is_err());
        assert!(
            DataAreaType::Dec {
                digits: 3,
                decimals: 4
            }
            .check()
            .is_err()
        );
    }
}
