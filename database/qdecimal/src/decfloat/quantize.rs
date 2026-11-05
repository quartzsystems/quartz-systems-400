// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QUANTIZE` and the comparisons of `COMPARE_DECFLOAT` and `TOTALORDER`.

use std::cmp::Ordering;

use super::{DecFloat, DfKind, DfRounding, invalid};
use crate::decimal::digits;
use crate::error::Result;

/// `a` with the exponent of `b`, rounded in `mode`, in the wider format.
/// A result whose coefficient has too many digits, or infinity beside a
/// number, is NaN, as IBM i returns it; a signaling NaN is invalid.
pub fn quantize(a: &DecFloat, b: &DecFloat, mode: DfRounding) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    let nan = DecFloat::special(DfKind::NaN, false, format);
    if a.kind == DfKind::SNaN || b.kind == DfKind::SNaN {
        return Err(invalid("a signaling NaN operand"));
    }
    if a.kind == DfKind::NaN {
        return Ok(DecFloat { format, ..*a });
    }
    if b.kind == DfKind::NaN {
        return Ok(DecFloat { format, ..*b });
    }
    match (a.kind, b.kind) {
        (DfKind::Infinity, DfKind::Infinity) => return Ok(DecFloat { format, ..*a }),
        (DfKind::Infinity, _) | (_, DfKind::Infinity) => return Ok(nan),
        _ => {}
    }
    let e = b.exp;
    if e < format.etiny() || e > format.qmax() {
        return Ok(nan);
    }
    let mag = digits::from_u128(a.coeff);
    let mag = match a.exp.cmp(&e) {
        Ordering::Equal => mag,
        Ordering::Greater => digits::shl(&mag, usize::try_from(a.exp - e).expect("positive")),
        Ordering::Less => {
            let n = usize::try_from(e - a.exp).expect("positive");
            let (kept, dropped) = digits::shr(&mag, n);
            if mode.rounds_up(a.neg, kept.first().copied().unwrap_or(0), &dropped) {
                digits::add_one(&kept)
            } else {
                kept
            }
        }
    };
    let mut mag = mag;
    digits::trim(&mut mag);
    if mag.len() > format.digits() as usize {
        return Ok(nan);
    }
    let coeff = digits::to_u128(&mag).expect("at most 34 digits");
    Ok(DecFloat { neg: a.neg, kind: DfKind::Finite, coeff, exp: e, format })
}

/// `COMPARE_DECFLOAT`: 0 equal in value and exponent, 1 `a` less, 2
/// `a` greater (in the total order, so `2.0 > 2.00`), 3 unordered (a
/// NaN).
#[must_use]
pub fn compare_total(a: &DecFloat, b: &DecFloat) -> u8 {
    if a.is_nan() || b.is_nan() {
        return 3;
    }
    match a.total_cmp(b) {
        Ordering::Equal => 0,
        Ordering::Less => 1,
        Ordering::Greater => 2,
    }
}
