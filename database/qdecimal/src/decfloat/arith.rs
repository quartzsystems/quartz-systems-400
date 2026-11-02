// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Addition, subtraction, multiplication and division, each rounded
//! half-even to the wider operand's format. A NaN operand gives NaN; a
//! signaling NaN, `Infinity - Infinity`, `0 * Infinity`, `0 / 0` and
//! `Infinity / Infinity` are invalid operations; a nonzero over zero is
//! division by zero.

use super::{DecFloat, DfFormat, DfKind, invalid};
use crate::decimal::digits::{self, Digits};
use crate::error::{DecimalError, Result};

/// NaN operands: an error for a signaling one, else the first NaN.
fn nans(a: &DecFloat, b: &DecFloat, format: DfFormat) -> Result<Option<DecFloat>> {
    if a.kind == DfKind::SNaN || b.kind == DfKind::SNaN {
        return Err(invalid("a signaling NaN operand"));
    }
    for v in [a, b] {
        if v.kind == DfKind::NaN {
            return Ok(Some(DecFloat { format, ..*v }));
        }
    }
    Ok(None)
}

fn mag(v: &DecFloat) -> Digits {
    digits::from_u128(v.coeff)
}

fn top(m: &Digits, e: i32) -> i32 {
    e + i32::try_from(m.len()).expect("small")
}

/// `a + b`.
pub fn add(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    match (a.kind, b.kind) {
        (DfKind::Infinity, DfKind::Infinity) if a.neg != b.neg => return Err(invalid("Infinity - Infinity")),
        (DfKind::Infinity, _) => return Ok(DecFloat { format, ..*a }),
        (_, DfKind::Infinity) => return Ok(DecFloat { format, ..*b }),
        _ => {}
    }
    let (mut ma, mut ea) = (mag(a), a.exp);
    let (mut mb, mut eb) = (mag(b), b.exp);
    // An operand far below the other only decides the rounding: a
    // sticky digit stands for it.
    let p = i32::try_from(format.digits()).expect("small");
    let far = |m: &Digits, e: i32, o: &Digits, oe: i32| !m.is_empty() && !o.is_empty() && top(m, e) < top(o, oe) - p - 3;
    if far(&mb, eb, &ma, ea) {
        eb = eb.max(ea.min(top(&ma, ea) - p - 4) - 1);
        mb = vec![1];
    } else if far(&ma, ea, &mb, eb) {
        ea = ea.max(eb.min(top(&mb, eb) - p - 4) - 1);
        ma = vec![1];
    }
    let e = ea.min(eb);
    let wa = digits::shl(&ma, usize::try_from(ea - e).expect("aligned"));
    let wb = digits::shl(&mb, usize::try_from(eb - e).expect("aligned"));
    let (neg, sum) = if a.neg == b.neg {
        (a.neg, digits::add(&wa, &wb))
    } else {
        match digits::cmp(&wa, &wb) {
            std::cmp::Ordering::Less => (b.neg, digits::sub(&wb, &wa)),
            std::cmp::Ordering::Greater => (a.neg, digits::sub(&wa, &wb)),
            // An exact zero is positive, unless both were negative.
            std::cmp::Ordering::Equal => (false, Vec::new()),
        }
    };
    let neg = if sum.is_empty() { a.neg && b.neg } else { neg };
    DecFloat::round(neg, &sum, e, format)
}

/// `a - b`.
pub fn sub(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    if b.kind == DfKind::NaN || b.kind == DfKind::SNaN {
        return add(a, b);
    }
    add(a, &b.negated())
}

/// `a * b`.
pub fn mul(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    let neg = a.neg != b.neg;
    if a.kind == DfKind::Infinity || b.kind == DfKind::Infinity {
        if a.is_zero() || b.is_zero() {
            return Err(invalid("0 * Infinity"));
        }
        return Ok(DecFloat::special(DfKind::Infinity, neg, format));
    }
    DecFloat::round(neg, &digits::mul(&mag(a), &mag(b)), a.exp + b.exp, format)
}

/// `a / b`.
pub fn div(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    let neg = a.neg != b.neg;
    match (a.kind, b.kind) {
        (DfKind::Infinity, DfKind::Infinity) => return Err(invalid("Infinity / Infinity")),
        (DfKind::Infinity, _) => return Ok(DecFloat::special(DfKind::Infinity, neg, format)),
        // A number over infinity is zero at the smallest exponent.
        (_, DfKind::Infinity) => return DecFloat::round(neg, &[], format.etiny(), format),
        _ => {}
    }
    if b.is_zero() {
        return if a.is_zero() { Err(invalid("0 / 0")) } else { Err(DecimalError::DivideByZero) };
    }
    let ideal = a.exp - b.exp;
    if a.is_zero() {
        return DecFloat::round(neg, &[], ideal, format);
    }
    let (ma, mb) = (mag(a), mag(b));
    // Enough quotient digits for the rounding digit and a sticky one.
    let p = format.digits() as usize;
    let shift = (p + 2 + mb.len()).saturating_sub(ma.len()) + 1;
    let (mut quot, rem) = digits::divmod(&digits::shl(&ma, shift), &mb);
    let mut exp = ideal - i32::try_from(shift).expect("small");
    if rem.is_empty() {
        // Exact: as close to the ideal exponent as the digits allow.
        while exp < ideal && quot.first() == Some(&0) {
            quot.remove(0);
            exp += 1;
        }
    } else {
        quot = digits::shl(&quot, 1);
        quot[0] = 1;
        exp -= 1;
    }
    DecFloat::round(neg, &quot, exp, format)
}
