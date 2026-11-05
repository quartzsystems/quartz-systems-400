// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A binary float's exact value as a `DECFLOAT`, as IBM i converts one:
//! every digit of the binary value, rounded to the format, trailing
//! zeros removed (`0.1E0` is `0.1000000000000000055511151231257827`,
//! `1E10` is `1E+10`).

use super::{DecFloat, DfFormat, DfKind, DfRounding};
use crate::decimal::digits::{self, Digits};
use crate::error::Result;

/// `m * 2^e` as decimal digits (low first) and a power of ten.
fn exact_digits(m: u64, e: i32) -> (Digits, i32) {
    let mut d = digits::from_u128(u128::from(m));
    if e >= 0 {
        for _ in 0..e {
            d = digits::mul(&d, &[2]);
        }
        (d, 0)
    } else {
        // m / 2^k = m * 5^k / 10^k.
        for _ in 0..-e {
            d = digits::mul(&d, &[5]);
        }
        (d, e)
    }
}

impl DecFloat {
    /// A double's exact value, rounded in `mode`, normalized.
    pub fn from_f64_exact(f: f64, format: DfFormat, mode: DfRounding) -> Result<Self> {
        if f.is_nan() {
            return Ok(Self::special(DfKind::NaN, false, format));
        }
        if f.is_infinite() {
            return Ok(Self::special(DfKind::Infinity, f < 0.0, format));
        }
        let bits = f.to_bits();
        let neg = bits >> 63 == 1;
        let raw_exp = i32::try_from((bits >> 52) & 0x7FF).expect("11 bits");
        let frac = bits & ((1 << 52) - 1);
        let (m, e) = if raw_exp == 0 { (frac, -1074) } else { (frac | (1 << 52), raw_exp - 1075) };
        if m == 0 {
            return Ok(Self { neg, ..Self::zero(format) });
        }
        let (d, exp) = exact_digits(m, e);
        Ok(Self::round_in(neg, &d, exp, format, mode)?.normalized())
    }

    /// A single's exact value, as [`Self::from_f64_exact`].
    pub fn from_f32_exact(f: f32, format: DfFormat, mode: DfRounding) -> Result<Self> {
        Self::from_f64_exact(f64::from(f), format, mode)
    }
}
