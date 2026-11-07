// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Floating point fields: IEEE single or double, big-endian.

use super::check_len;
use crate::decimal::Decimal;
use crate::error::{DecimalError, Result};

/// A DDS `F` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Float {
    double: bool,
}

impl Float {
    /// `FLTPCN(*SINGLE)`.
    pub const SINGLE: Self = Self { double: false };
    /// `FLTPCN(*DOUBLE)`.
    pub const DOUBLE: Self = Self { double: true };

    /// 4 or 8 bytes.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        if self.double { 8 } else { 4 }
    }

    /// Encode: the nearest float to the value.
    pub fn encode(&self, value: &Decimal) -> Result<Vec<u8>> {
        let text = value.to_string();
        Ok(if self.double {
            let f: f64 = text.parse().expect("decimal text parses as a float");
            f.to_be_bytes().to_vec()
        } else {
            let f: f32 = text.parse().expect("decimal text parses as a float");
            f.to_be_bytes().to_vec()
        })
    }

    /// Encode a binary float as it is; a single rounds to the nearest
    /// single, and a finite value past its range is an error.
    pub fn encode_f64(&self, value: f64) -> Result<Vec<u8>> {
        if self.double {
            return Ok(value.to_be_bytes().to_vec());
        }
        #[allow(clippy::cast_possible_truncation)] // rounding to a single is the point
        let f = value as f32;
        if f.is_infinite() && value.is_finite() {
            return Err(DecimalError::SizeOverflow {
                value: format!("{value:e}"),
                digits: 0,
                scale: 0,
            });
        }
        Ok(f.to_be_bytes().to_vec())
    }

    /// Decode to a binary float, special values included.
    pub fn decode_f64(&self, bytes: &[u8]) -> Result<f64> {
        check_len(bytes, self.byte_len())?;
        Ok(if self.double {
            f64::from_be_bytes(bytes.try_into().expect("8 bytes"))
        } else {
            f64::from(f32::from_be_bytes(bytes.try_into().expect("4 bytes")))
        })
    }

    /// Decode: the shortest decimal that reads back as the same float.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decimal> {
        check_len(bytes, self.byte_len())?;
        let text = if self.double {
            let f = f64::from_be_bytes(bytes.try_into().expect("8 bytes"));
            finite(f.is_nan(), f.is_infinite())?;
            format!("{f}")
        } else {
            let f = f32::from_be_bytes(bytes.try_into().expect("4 bytes"));
            finite(f.is_nan(), f.is_infinite())?;
            format!("{f}")
        };
        Decimal::parse(&text)
    }
}

fn finite(nan: bool, inf: bool) -> Result<()> {
    if nan {
        Err(DecimalError::NotFinite { what: "NaN" })
    } else if inf {
        Err(DecimalError::NotFinite { what: "infinity" })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn round_trips() {
        let f = Float::SINGLE;
        assert_eq!(f.byte_len(), 4);
        assert_eq!(f.encode(&d("1.5")).unwrap(), 1.5f32.to_be_bytes());
        assert_eq!(f.decode(&f.encode(&d("0.1")).unwrap()).unwrap(), d("0.1"));
        assert_eq!(
            f.decode(&f.encode(&d("-123456")).unwrap()).unwrap(),
            d("-123456")
        );
        let g = Float::DOUBLE;
        assert_eq!(g.byte_len(), 8);
        assert_eq!(
            g.decode(&g.encode(&d("12345678.90123")).unwrap()).unwrap(),
            d("12345678.90123")
        );
        assert_eq!(g.decode(&0.0f64.to_be_bytes()).unwrap(), d("0"));
        assert!(g.decode(&f64::NAN.to_be_bytes()).is_err());
        assert!(g.decode(&f64::INFINITY.to_be_bytes()).is_err());
        assert!(f.decode(&[0; 8]).is_err());
    }

    #[test]
    fn binary_floats() {
        let g = Float::DOUBLE;
        let back = |f: Float, x: f64| f.decode_f64(&f.encode_f64(x).unwrap()).unwrap().to_bits();
        assert_eq!(back(g, 1e300), 1e300f64.to_bits());
        assert_eq!(back(g, -2.5e-308), (-2.5e-308f64).to_bits());
        let f = Float::SINGLE;
        assert_eq!(back(f, 1e-45), f64::from(1e-45f32).to_bits());
        assert!(f.encode_f64(1e300).is_err());
        assert!(
            f.decode_f64(&f.encode_f64(f64::INFINITY).unwrap())
                .unwrap()
                .is_infinite()
        );
    }
}
