// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Binary fields (`B` with digits and decimals, big-endian two's
//! complement) and the plain integers RPG calls `I` and `U`.

use super::{bad, check_len};
use crate::decimal::{Decimal, Rounding, Shape};
use crate::error::{DecimalError, Result};

/// A DDS `B` field: a two's complement integer with implied decimals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binary {
    shape: Shape,
}

impl Binary {
    /// A field of `digits` (1 to 18) with `scale` decimal positions.
    pub fn new(digits: u8, scale: u8) -> Result<Self> {
        let shape = Shape::new(digits, scale)?;
        if digits > 18 {
            return Err(DecimalError::Precision {
                digits,
                scale,
                why: "a binary field has at most 18 digits",
            });
        }
        Ok(Self { shape })
    }

    /// The shape.
    #[must_use]
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// 2 bytes up to 4 digits, 4 up to 9, 8 beyond.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        match self.shape.digits {
            0..=4 => 2,
            5..=9 => 4,
            _ => 8,
        }
    }

    /// Encode, fitting the value to the declared digits.
    pub fn encode(&self, value: &Decimal, rounding: Rounding) -> Result<Vec<u8>> {
        let mut out = vec![0u8; self.byte_len()];
        self.encode_into(value, rounding, &mut out)?;
        Ok(out)
    }

    /// Encode into a buffer of exactly [`byte_len`](Self::byte_len) bytes.
    pub fn encode_into(&self, value: &Decimal, rounding: Rounding, out: &mut [u8]) -> Result<()> {
        check_len(out, self.byte_len())?;
        let v = value.fit(self.shape, rounding)?;
        let m = v.mantissa_i128().expect("at most 18 digits");
        let bytes = m.to_be_bytes();
        out.copy_from_slice(&bytes[16 - out.len()..]);
        Ok(())
    }

    /// Decode. The value may exceed the declared digits; it is returned
    /// as held.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decimal> {
        check_len(bytes, self.byte_len())?;
        let m = match bytes.len() {
            2 => i128::from(i16::from_be_bytes([bytes[0], bytes[1]])),
            4 => i128::from(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
            _ => i128::from(i64::from_be_bytes(bytes.try_into().expect("8 bytes"))),
        };
        Decimal::new(m, self.shape.scale)
    }
}

/// A plain integer of 1, 2, 4 or 8 bytes, signed or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Integer {
    bytes: u8,
    signed: bool,
}

impl Integer {
    /// A field of `bytes` (1, 2, 4 or 8).
    pub fn new(bytes: u8, signed: bool) -> Result<Self> {
        if !matches!(bytes, 1 | 2 | 4 | 8) {
            return Err(bad(format!(
                "an integer field has 1, 2, 4 or 8 bytes, not {bytes}"
            )));
        }
        Ok(Self { bytes, signed })
    }

    /// Bytes in the record.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        usize::from(self.bytes)
    }

    /// Whether it carries a sign.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        self.signed
    }

    /// The range as a shape for messages: digits of the largest value.
    fn shape(self) -> Shape {
        let digits = match (self.bytes, self.signed) {
            (1, _) => 3,
            (2, _) => 5,
            (4, _) => 10,
            (_, true) => 19,
            (_, false) => 20,
        };
        Shape { digits, scale: 0 }
    }

    fn range(self) -> (i128, i128) {
        let bits = u32::from(self.bytes) * 8;
        if self.signed {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        }
    }

    /// Encode the integer part of `value`, toward zero.
    pub fn encode(&self, value: &Decimal) -> Result<Vec<u8>> {
        let (lo, hi) = self.range();
        let overflow = || DecimalError::SizeOverflow {
            value: value.to_string(),
            digits: self.shape().digits,
            scale: 0,
        };
        let m = value.to_i128().ok_or_else(overflow)?;
        if m < lo || m > hi {
            return Err(overflow());
        }
        let bytes = m.to_be_bytes();
        Ok(bytes[16 - self.byte_len()..].to_vec())
    }

    /// Decode.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decimal> {
        check_len(bytes, self.byte_len())?;
        let mut wide = if self.signed && bytes[0] & 0x80 != 0 {
            [0xFFu8; 16]
        } else {
            [0u8; 16]
        };
        wide[16 - bytes.len()..].copy_from_slice(bytes);
        Decimal::new(i128::from_be_bytes(wide), 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn binary_fields() {
        let b = Binary::new(4, 0).unwrap();
        assert_eq!(b.byte_len(), 2);
        assert_eq!(
            b.encode(&d("1234"), Rounding::Truncate).unwrap(),
            [0x04, 0xD2]
        );
        assert_eq!(
            b.encode(&d("-1"), Rounding::Truncate).unwrap(),
            [0xFF, 0xFF]
        );
        assert!(b.encode(&d("12345"), Rounding::Truncate).is_err());
        assert_eq!(b.decode(&[0x7F, 0xFF]).unwrap(), d("32767"));
        let b = Binary::new(9, 2).unwrap();
        assert_eq!(b.byte_len(), 4);
        assert_eq!(
            b.encode(&d("-12345.678"), Rounding::HalfAdjust).unwrap(),
            (-1_234_568i32).to_be_bytes()
        );
        assert_eq!(
            b.decode(&b.encode(&d("0.01"), Rounding::Truncate).unwrap())
                .unwrap(),
            d("0.01")
        );
        let b = Binary::new(18, 0).unwrap();
        assert_eq!(b.byte_len(), 8);
        let v = d("-123456789012345678");
        assert_eq!(
            b.decode(&b.encode(&v, Rounding::Truncate).unwrap())
                .unwrap(),
            v
        );
        assert!(Binary::new(19, 0).is_err());
    }

    #[test]
    fn integers() {
        let i = Integer::new(2, true).unwrap();
        assert_eq!(i.encode(&d("-2")).unwrap(), [0xFF, 0xFE]);
        assert_eq!(i.decode(&[0xFF, 0xFE]).unwrap(), d("-2"));
        assert_eq!(i.encode(&d("7.9")).unwrap(), [0x00, 0x07]);
        assert!(i.encode(&d("32768")).is_err());
        let u = Integer::new(1, false).unwrap();
        assert_eq!(u.decode(&[0xFF]).unwrap(), d("255"));
        assert!(u.encode(&d("-1")).is_err());
        assert!(u.encode(&d("256")).is_err());
        let u8w = Integer::new(8, false).unwrap();
        assert_eq!(u8w.decode(&[0xFF; 8]).unwrap(), d("18446744073709551615"));
        let i8w = Integer::new(8, true).unwrap();
        assert_eq!(
            i8w.decode(&[0x80, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            d("-9223372036854775808")
        );
        assert!(Integer::new(3, true).is_err());
    }
}
