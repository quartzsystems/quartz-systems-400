// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Addition, subtraction, multiplication and division, each rounded to
//! the wider operand's format in a rounding mode (half-even for the
//! plain forms). A NaN operand gives NaN; a signaling NaN is an invalid
//! operation; `Infinity - Infinity`, `0 * Infinity` and `Infinity /
//! Infinity` give NaN, as IBM i returns them; anything over zero (`0 /
//! 0` too) is division by zero.

use super::{DecFloat, DfFormat, DfKind, DfRounding, invalid};
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

fn nan(format: DfFormat) -> DecFloat {
    DecFloat::special(DfKind::NaN, false, format)
}

/// `a + b`, half-even.
pub fn add(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    add_in(a, b, DfRounding::HalfEven)
}

/// `a - b`, half-even.
pub fn sub(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    sub_in(a, b, DfRounding::HalfEven)
}

/// `a * b`, half-even.
pub fn mul(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    mul_in(a, b, DfRounding::HalfEven)
}

/// `a / b`, half-even.
pub fn div(a: &DecFloat, b: &DecFloat) -> Result<DecFloat> {
    div_in(a, b, DfRounding::HalfEven)
}

/// `a + b` in `mode`.
pub fn add_in(a: &DecFloat, b: &DecFloat, mode: DfRounding) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    match (a.kind, b.kind) {
        (DfKind::Infinity, DfKind::Infinity) if a.neg != b.neg => return Ok(nan(format)),
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
            // An exact zero is positive, unless both were negative (or
            // the mode rounds towards negative infinity).
            std::cmp::Ordering::Equal => (mode == DfRounding::Floor, Vec::new()),
        }
    };
    let neg = if sum.is_empty() { (a.neg && b.neg) || neg } else { neg };
    DecFloat::round_arith(neg, &sum, e, format, mode)
}

/// `a - b` in `mode`.
pub fn sub_in(a: &DecFloat, b: &DecFloat, mode: DfRounding) -> Result<DecFloat> {
    if b.kind == DfKind::NaN || b.kind == DfKind::SNaN {
        return add_in(a, b, mode);
    }
    add_in(a, &b.negated(), mode)
}

/// `a * b` in `mode`.
pub fn mul_in(a: &DecFloat, b: &DecFloat, mode: DfRounding) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    let neg = a.neg != b.neg;
    if a.kind == DfKind::Infinity || b.kind == DfKind::Infinity {
        if a.is_zero() || b.is_zero() {
            return Ok(nan(format));
        }
        return Ok(DecFloat::special(DfKind::Infinity, neg, format));
    }
    DecFloat::round_arith(neg, &digits::mul(&mag(a), &mag(b)), a.exp + b.exp, format, mode)
}

/// `a / b` in `mode`.
pub fn div_in(a: &DecFloat, b: &DecFloat, mode: DfRounding) -> Result<DecFloat> {
    let format = a.format.max(b.format);
    if let Some(n) = nans(a, b, format)? {
        return Ok(n);
    }
    let neg = a.neg != b.neg;
    match (a.kind, b.kind) {
        (DfKind::Infinity, DfKind::Infinity) => return Ok(nan(format)),
        (DfKind::Infinity, _) => return Ok(DecFloat::special(DfKind::Infinity, neg, format)),
        // A number over infinity is zero at the smallest exponent.
        (_, DfKind::Infinity) => return DecFloat::round_in(neg, &[], format.etiny(), format, mode),
        _ => {}
    }
    if b.is_zero() {
        return Err(DecimalError::DivideByZero);
    }
    let ideal = a.exp - b.exp;
    if a.is_zero() {
        return DecFloat::round_in(neg, &[], ideal, format, mode);
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
    DecFloat::round_arith(neg, &quot, exp, format, mode)
}
