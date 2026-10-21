// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Packed decimal: two digits per byte, the sign in the last nibble.

use super::{bad, check_len, sign_of};
use crate::decimal::{Decimal, Rounding, Shape};
use crate::error::Result;

/// A packed decimal field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Packed {
    shape: Shape,
}

impl Packed {
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

    /// Bytes in the record: `digits / 2 + 1`.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        usize::from(self.shape.digits) / 2 + 1
    }

    /// Nibbles that hold digits; one more than `digits` when that is even.
    fn digit_nibbles(self) -> usize {
        self.byte_len() * 2 - 1
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
        let mag = v.digits_le();
        let n = self.digit_nibbles();
        out.fill(0);
        // Nibble k of n counts from the left; digit place i sits at nibble n - 1 - i.
        for (i, &d) in mag.iter().enumerate() {
            let k = n - 1 - i;
            out[k / 2] |= if k.is_multiple_of(2) { d << 4 } else { d };
        }
        let sign = if v.is_negative() { 0xD } else { 0xC };
        out[self.byte_len() - 1] |= sign;
        Ok(())
    }

    /// Decode. Any nibble above 9, a nonzero pad nibble or a bad sign is
    /// a decimal data error.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decimal> {
        check_len(bytes, self.byte_len())?;
        let n = self.digit_nibbles();
        let nibble = |k: usize| {
            let b = bytes[k / 2];
            if k.is_multiple_of(2) { b >> 4 } else { b & 0xF }
        };
        let pad = n - usize::from(self.shape.digits);
        let mut mag = Vec::with_capacity(n);
        for k in (0..n).rev() {
            let d = nibble(k);
            if d > 9 {
                return Err(bad(format!("digit nibble {k} is {d:#X}")));
            }
            if k < pad && d != 0 {
                return Err(bad(format!("pad nibble is {d:#X}")));
            }
            mag.push(d);
        }
        let s = bytes[self.byte_len() - 1] & 0xF;
        let Some(neg) = sign_of(s) else {
            return Err(bad(format!("sign nibble is {s:#X}")));
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
        let p = Packed::new(7, 2).unwrap();
        assert_eq!(p.byte_len(), 4);
        assert_eq!(
            p.encode(&d("12345.67"), Rounding::Truncate).unwrap(),
            [0x12, 0x34, 0x56, 0x7C]
        );
        assert_eq!(
            p.encode(&d("-1.5"), Rounding::Truncate).unwrap(),
            [0x00, 0x00, 0x15, 0x0D]
        );
        assert_eq!(
            p.encode(&d("0"), Rounding::Truncate).unwrap(),
            [0, 0, 0, 0x0C]
        );
        let even = Packed::new(4, 0).unwrap();
        assert_eq!(even.byte_len(), 3);
        assert_eq!(
            even.encode(&d("1234"), Rounding::Truncate).unwrap(),
            [0x01, 0x23, 0x4C]
        );
        let one = Packed::new(1, 0).unwrap();
        assert_eq!(one.encode(&d("-9"), Rounding::Truncate).unwrap(), [0x9D]);
        let big = Packed::new(63, 0).unwrap();
        assert_eq!(big.byte_len(), 32);
        let v = d(&"7".repeat(63));
        assert_eq!(
            big.decode(&big.encode(&v, Rounding::Truncate).unwrap())
                .unwrap(),
            v
        );
    }

    #[test]
    fn rounding_and_overflow() {
        let p = Packed::new(5, 2).unwrap();
        assert_eq!(
            p.decode(&p.encode(&d("1.239"), Rounding::HalfAdjust).unwrap())
                .unwrap(),
            d("1.24")
        );
        assert_eq!(
            p.decode(&p.encode(&d("1.239"), Rounding::Truncate).unwrap())
                .unwrap(),
            d("1.23")
        );
        assert!(p.encode(&d("1000"), Rounding::Truncate).is_err());
        assert!(
            p.encode_into(&d("1"), Rounding::Truncate, &mut [0u8; 2])
                .is_err()
        );
    }

    #[test]
    fn decode_accepts_and_rejects() {
        let p = Packed::new(3, 1).unwrap();
        assert_eq!(p.decode(&[0x12, 0x3F]).unwrap(), d("12.3"));
        assert_eq!(p.decode(&[0x12, 0x3A]).unwrap(), d("12.3"));
        assert_eq!(p.decode(&[0x12, 0x3B]).unwrap(), d("-12.3"));
        assert_eq!(p.decode(&[0x00, 0x0D]).unwrap(), d("0"));
        assert!(p.decode(&[0x1A, 0x3C]).is_err());
        assert!(p.decode(&[0x12, 0x39]).is_err());
        assert!(p.decode(&[0x12]).is_err());
        let even = Packed::new(2, 0).unwrap();
        assert!(even.decode(&[0x11, 0x2C]).is_err());
        assert_eq!(even.decode(&[0x01, 0x2C]).unwrap(), d("12"));
    }
}
