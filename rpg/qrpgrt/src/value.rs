// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operands as the compiler describes them to the runtime library: an
//! address and a descriptor word (kind in the top byte, length or
//! digits in bits 8 to 55, decimals in the low byte).

use qdecimal::{Binary, Decimal, Float, Integer, Packed, Shape, Zoned};

/// What a descriptor names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Fixed character.
    Char,
    /// Varying character, 2-byte prefix.
    VarChar,
    /// Packed decimal.
    Packed,
    /// Zoned decimal.
    Zoned,
    /// Binary with decimals.
    Bin,
    /// Signed integer.
    Int,
    /// Unsigned integer.
    Uns,
    /// Indicator.
    Ind,
    /// Date.
    Date,
    /// Time.
    Time,
    /// Timestamp.
    Timestamp,
    /// Float.
    Float,
}

/// A decoded descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Desc {
    /// The kind.
    pub kind: Kind,
    /// Length (characters) or digits (numbers) or bytes (floats).
    pub len: u32,
    /// Decimal positions.
    pub dec: u8,
}

impl Desc {
    /// From the word.
    #[must_use]
    pub fn of(word: i64) -> Self {
        let w = word.cast_unsigned();
        let kind = match w >> 56 {
            2 => Kind::VarChar,
            3 => Kind::Packed,
            4 => Kind::Zoned,
            5 => Kind::Bin,
            6 => Kind::Int,
            7 => Kind::Uns,
            8 => Kind::Ind,
            9 => Kind::Date,
            10 => Kind::Time,
            11 => Kind::Timestamp,
            12 => Kind::Float,
            _ => Kind::Char,
        };
        Self {
            kind,
            len: u32::try_from((w >> 8) & 0xFFFF_FFFF_FFFF).unwrap_or(0),
            dec: u8::try_from(w & 0xFF).unwrap_or(0),
        }
    }

    /// Bytes of storage.
    #[must_use]
    pub fn size(&self) -> usize {
        let n = usize::try_from(self.len).unwrap_or(0);
        match self.kind {
            Kind::VarChar => n + 2,
            Kind::Packed => n / 2 + 1,
            Kind::Bin => {
                if n <= 4 {
                    2
                } else if n <= 9 {
                    4
                } else {
                    8
                }
            }
            Kind::Int | Kind::Uns => match n {
                3 => 1,
                5 => 2,
                10 => 4,
                _ => 8,
            },
            Kind::Ind => 1,
            _ => n,
        }
    }

    /// Whether it is a number.
    #[must_use]
    pub fn numeric(&self) -> bool {
        matches!(self.kind, Kind::Packed | Kind::Zoned | Kind::Bin | Kind::Int | Kind::Uns | Kind::Float)
    }

    /// The decimal shape of a number.
    #[must_use]
    pub fn shape(&self) -> Option<Shape> {
        let d = u8::try_from(self.len.min(63)).ok()?;
        match self.kind {
            Kind::Float => Shape::new(31, 15).ok(),
            _ => Shape::new(d.max(1), self.dec).ok(),
        }
    }
}

/// The bytes at an address.
#[must_use]
pub fn bytes(addr: u64, len: usize) -> &'static mut [u8] {
    mi::storage::view(addr, len)
}

/// A number from storage.
pub fn read_num(addr: u64, d: &Desc) -> Result<Decimal, mi::Cond> {
    let b = bytes(addr, d.size());
    let digits = u8::try_from(d.len.min(63)).unwrap_or(63);
    let r = match d.kind {
        Kind::Packed => Packed::new(digits, d.dec).and_then(|p| p.decode(b)),
        Kind::Zoned => Zoned::new(digits, d.dec).and_then(|z| z.decode(b)),
        Kind::Bin => Binary::new(digits, d.dec).and_then(|x| x.decode(b)),
        Kind::Int | Kind::Uns => Integer::new(u8::try_from(d.size()).unwrap_or(8), d.kind == Kind::Int).and_then(|i| i.decode(b)),
        Kind::Float => if d.len == 4 { Float::SINGLE } else { Float::DOUBLE }.decode(b),
        _ => return Err(mi::Cond::Mch1202),
    };
    r.map_err(|_| mi::Cond::Mch1202)
}

/// A float from storage.
#[must_use]
pub fn read_f64(addr: u64, d: &Desc) -> Option<f64> {
    if d.kind != Kind::Float {
        return None;
    }
    let b = bytes(addr, d.size());
    Some(if d.len == 4 {
        f64::from(f32::from_be_bytes(b[..4].try_into().ok()?))
    } else {
        f64::from_be_bytes(b[..8].try_into().ok()?)
    })
}

/// Write text into a varying character field (length, then data).
pub fn write_varchar(addr: u64, text: &[u8]) {
    let n = u16::try_from(text.len()).unwrap_or(u16::MAX);
    let b = bytes(addr, 2 + usize::from(n));
    b[..2].copy_from_slice(&n.to_be_bytes());
    b[2..].copy_from_slice(&text[..usize::from(n)]);
}

/// UTF-8 text as CP037, unmappable characters as blanks.
#[must_use]
pub fn ebcdic(s: &str) -> Vec<u8> {
    s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect()
}
