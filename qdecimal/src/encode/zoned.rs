// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Zoned decimal: one digit per byte under zone `F`, the sign in the
//! last byte's zone.

use super::{bad, check_len, sign_of};
use crate::decimal::{Decimal, Rounding, Shape};
use crate::error::Result;

/// A zoned decimal field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Zoned {
    shape: Shape,
}

impl Zoned {
    /// A field of `digits` with `scale` decimal positions.
    pub fn new(digits: u8, scale: u8) -> Result<Self> {
        Ok(Self {
            shape: Shape::new(digits, scale)?,
        })
    }

    /// The shape.
    #[must_use]
    pub fn shape(&self) -> Shape {
        self.shape
    }

    /// Bytes in the record: one per digit.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        usize::from(self.shape.digits)
    }

    /// Encode, fitting the value to the field.
    pub fn encode(&self, value: &Decimal, rounding: Rounding) -> Result<Vec<u8>> {
        let mut out = vec![0u8; self.byte_len()];
        self.encode_into(value, rounding, &mut out)?;
        Ok(out)
    }

    /// Encode into a buffer of exactly [`byte_len`](Self::byte_len) bytes.
    pub fn encode_into(&self, value: &Decimal, rounding: Rounding, out: &mut [u8]) -> Result<()> {
        check_len(out, self.byte_len())?;
        let v = value.fit(self.shape, rounding)?;
        let n = self.byte_len();
        out.fill(0xF0);
        for (i, &d) in v.digits_le().iter().enumerate() {
            out[n - 1 - i] |= d;
        }
        let sign = if v.is_negative() { 0xD0 } else { 0xC0 };
        out[n - 1] = sign | (out[n - 1] & 0x0F);
        Ok(())
    }

    /// Decode. A digit above 9, a zone other than `F` before the last
    /// byte, or a bad sign zone is a decimal data error.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decimal> {
        check_len(bytes, self.byte_len())?;
        let n = self.byte_len();
        let mut mag = Vec::with_capacity(n);
        for (k, &b) in bytes.iter().enumerate().rev() {
            let d = b & 0x0F;
            if d > 9 {
                return Err(bad(format!("digit of byte {k} is {d:#X}")));
            }
            if k != n - 1 && b >> 4 != 0xF {
                return Err(bad(format!("zone of byte {k} is {:#X}", b >> 4)));
            }
            mag.push(d);
        }
        let zone = bytes[n - 1] >> 4;
        let Some(neg) = sign_of(zone) else {
            return Err(bad(format!("sign zone is {zone:#X}")));
        };
        Decimal::from_digits(neg, mag, self.shape.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn layouts() {
        let z = Zoned::new(5, 2).unwrap();
        assert_eq!(z.byte_len(), 5);
        assert_eq!(
            z.encode(&d("123.45"), Rounding::Truncate).unwrap(),
            [0xF1, 0xF2, 0xF3, 0xF4, 0xC5]
        );
        assert_eq!(
            z.encode(&d("-0.5"), Rounding::Truncate).unwrap(),
            [0xF0, 0xF0, 0xF0, 0xF5, 0xD0]
        );
        assert_eq!(
            z.encode(&d("0"), Rounding::Truncate).unwrap(),
            [0xF0, 0xF0, 0xF0, 0xF0, 0xC0]
        );
        assert_eq!(
            z.decode(&[0xF1, 0xF2, 0xF3, 0xF4, 0xF5]).unwrap(),
            d("123.45")
        );
        assert_eq!(
            z.decode(&[0xF0, 0xF0, 0xF1, 0xF0, 0xB0]).unwrap(),
            d("-1.00")
        );
        assert!(z.encode(&d("1000"), Rounding::Truncate).is_err());
        let v = d(&format!("-{}", "3".repeat(63)));
        let big = Zoned::new(63, 0).unwrap();
        assert_eq!(
            big.decode(&big.encode(&v, Rounding::HalfAdjust).unwrap())
                .unwrap(),
            v
        );
    }

    #[test]
    fn decode_rejects() {
        let z = Zoned::new(3, 0).unwrap();
        assert!(z.decode(&[0x40, 0x40, 0x40]).is_err());
        assert!(z.decode(&[0xF1, 0xFA, 0xC3]).is_err());
        assert!(z.decode(&[0xF1, 0xF2, 0x93]).is_err());
        assert!(z.decode(&[0xF1, 0xF2]).is_err());
        assert_eq!(z.decode(&[0xF1, 0xF2, 0xE3]).unwrap(), d("123"));
    }
}
