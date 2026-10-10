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
}
