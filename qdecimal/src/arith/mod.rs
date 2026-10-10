// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arithmetic with the intermediate precision rules of the RPG runtime.
//! Operands carry a declared shape, as fields and literals do; the
//! result's shape follows the rules in `docs/DECIMAL.md`, and the value
//! is truncated to it.

use crate::decimal::{Decimal, MAX_DIGITS, Rounding, Shape, digits};
use crate::error::{DecimalError, Result};

/// Which precision rules apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// The default rules: decimal positions give way to integer digits.
    Default,
    /// `EXPROPTS(*RESDECPOS)` or the `R` extender: intermediate results
    /// keep at least the result field's decimal positions.
    ResDecPos {
        /// Decimal positions of the result field.
        result_scale: u8,
    },
}

/// A value with the shape of the field or literal it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typed {
    /// The value, at `shape.scale`.
    pub value: Decimal,
    /// The declared digits and decimals.
    pub shape: Shape,
}

impl Typed {
    /// A field's value: the value is fitted to the shape (truncated).
    pub fn new(value: &Decimal, shape: Shape) -> Result<Self> {
        Ok(Self {
            value: value.fit(shape, Rounding::Truncate)?,
            shape,
        })
    }

    /// A literal: its own digits are its shape.
    #[must_use]
    pub fn literal(value: Decimal) -> Self {
        let shape = value.shape();
        Self { value, shape }
    }

    /// The value assigned to a field, half-adjusted when asked.
    pub fn assign(&self, shape: Shape, rounding: Rounding) -> Result<Decimal> {
        self.value.fit(shape, rounding)
    }
}

/// The shape of an intermediate result from `integer` integer digits
/// and `scale` decimal positions, trimmed to 63 digits by the mode.
fn shape(integer: u16, scale: u16, mode: Mode, value: &Decimal) -> Result<Shape> {
    let max = u16::from(MAX_DIGITS);
    let overflow = || DecimalError::SizeOverflow {
        value: value.to_string(),
        digits: MAX_DIGITS,
        scale: u8::try_from(scale.min(max)).expect("fits"),
    };
    let floor = match mode {
        Mode::Default => 0,
        Mode::ResDecPos { result_scale } => u16::from(result_scale).min(scale),
    };
    let mut integer = integer;
    let mut scale = scale;
    if integer + scale > max {
        scale = max.saturating_sub(integer).max(floor);
        if integer + scale > max {
            integer = max - scale;
        }
    }
    if integer > max {
        return Err(overflow());
    }
    let digits = u8::try_from(integer + scale).expect("at most 63");
    let scale = u8::try_from(scale).expect("at most 63");
    Shape::new(digits.max(1), scale)
}

/// Truncate an exact result to its intermediate shape.
fn settle(exact: &Decimal, shape: Shape) -> Result<Typed> {
    let value = exact.fit(shape, Rounding::Truncate)?;
    Ok(Typed { value, shape })
}

/// `a + b`: `max(i1, i2) + 1` integer digits, `max(d1, d2)` decimals.
pub fn add(a: &Typed, b: &Typed, mode: Mode) -> Result<Typed> {
    let integer = u16::from(a.shape.integer().max(b.shape.integer())) + 1;
    let scale = u16::from(a.shape.scale.max(b.shape.scale));
    let exact = a.value.add_exact(&b.value)?;
    let shape = shape(integer, scale, mode, &exact)?;
    settle(&exact, shape)
}

/// `a - b`: the same shape as addition.
pub fn sub(a: &Typed, b: &Typed, mode: Mode) -> Result<Typed> {
    let negated = Typed {
        value: b.value.negated(),
        shape: b.shape,
    };
    add(a, &negated, mode)
}

/// `a * b`: `i1 + i2` integer digits, `d1 + d2` decimals.
pub fn mul(a: &Typed, b: &Typed, mode: Mode) -> Result<Typed> {
    let integer = u16::from(a.shape.integer()) + u16::from(b.shape.integer());
    let scale = u16::from(a.shape.scale) + u16::from(b.shape.scale);
    let shape = shape(integer, scale, mode, &a.value)?;
    // The exact product may carry more decimals than any Decimal holds,
    // so multiply the digit vectors and drop the excess at once.
    let mag = digits::mul(a.value.digits_le(), b.value.digits_le());
    let total_scale = u16::from(a.value.scale()) + u16::from(b.value.scale());
    let drop = usize::from(total_scale.saturating_sub(u16::from(shape.scale)));
    let (kept, _) = digits::shr(&mag, drop);
    let neg = a.value.is_negative() != b.value.is_negative();
    let exact = Decimal::from_digits(neg, kept, shape.scale)?;
    settle(&exact, shape)
}

/// `a / b`: `i1 + d2` integer digits and every remaining position as
/// decimals, `63 - (i1 + d2)`.
pub fn div(a: &Typed, b: &Typed, mode: Mode) -> Result<Typed> {
    if b.value.is_zero() {
        return Err(DecimalError::DivideByZero);
    }
    let integer = u16::from(a.shape.integer()) + u16::from(b.shape.scale);
    let scale = u16::from(MAX_DIGITS).saturating_sub(integer);
    let shape = shape(integer, scale, mode, &a.value)?;
    let exact = a.value.div_to_scale(&b.value, shape.scale)?;
    settle(&exact, shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str, digits: u8, scale: u8) -> Typed {
        Typed::new(
            &Decimal::parse(s).unwrap(),
            Shape::new(digits, scale).unwrap(),
        )
        .unwrap()
    }

    fn lit(s: &str) -> Typed {
        Typed::literal(Decimal::parse(s).unwrap())
    }

    #[test]
    fn shapes_follow_the_rules() {
        let r = add(&t("123.45", 5, 2), &t("1.234", 7, 3), Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 8,
                scale: 3
            }
        );
        assert_eq!(r.value.to_string(), "124.684");
        let r = sub(&t("1", 1, 0), &t("2", 1, 0), Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 2,
                scale: 0
            }
        );
        assert_eq!(r.value.to_string(), "-1");
        let r = mul(&t("12.5", 3, 1), &t("-2.25", 3, 2), Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 6,
                scale: 3
            }
        );
        assert_eq!(r.value.to_string(), "-28.125");
        let r = div(&t("1", 1, 0), &t("3", 1, 0), Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 62
            }
        );
        assert!(r.value.to_string().starts_with("0.3333333333"));
        let r = div(&t("100.0", 5, 1), &t("0.25", 3, 2), Mode::Default).unwrap();
        assert_eq!(r.shape.integer(), 6);
        assert_eq!(r.shape.scale, 57);
        assert_eq!(r.value.to_i128(), Some(400));
        assert_eq!(
            div(&lit("1"), &lit("0"), Mode::Default),
            Err(DecimalError::DivideByZero)
        );
    }

    #[test]
    fn sixty_three_digit_limit() {
        let a = t(&"9".repeat(31), 31, 0);
        let b = t(&"9".repeat(32), 32, 0);
        let r = mul(&a, &b, Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 0
            }
        );
        let big = t(&"1".repeat(40), 40, 0);
        assert!(matches!(
            mul(&big, &big, Mode::Default),
            Err(DecimalError::SizeOverflow { .. })
        ));
        // Decimals give way first.
        let a = t("1.5", 40, 20);
        let b = t("2.5", 40, 20);
        let r = mul(&a, &b, Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 23
            }
        );
        assert_eq!(r.value.to_string(), format!("3.75{}", "0".repeat(21)));
        let r = add(&t("1", 63, 0), &t("1", 63, 0), Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 0
            }
        );
        assert_eq!(r.value.to_string(), "2");
    }

    #[test]
    fn resdecpos_keeps_result_decimals() {
        let a = t("1", 40, 30);
        let b = t("3", 30, 0);
        let r = div(&a, &b, Mode::Default).unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 53
            }
        );
        let r = mul(&t("1", 40, 20), &t("1", 40, 20), Mode::Default).unwrap();
        assert_eq!(r.shape.scale, 23);
        let r = mul(
            &t("1", 40, 20),
            &t("1", 40, 20),
            Mode::ResDecPos { result_scale: 30 },
        )
        .unwrap();
        assert_eq!(
            r.shape,
            Shape {
                digits: 63,
                scale: 30
            }
        );
        assert_eq!(r.shape.integer(), 33);
        let r = mul(
            &t("1", 40, 20),
            &t("1", 40, 20),
            Mode::ResDecPos { result_scale: 2 },
        )
        .unwrap();
        assert_eq!(r.shape.scale, 23);
    }

    #[test]
    fn assignment_rounds_and_overflows() {
        let r = div(&lit("2"), &lit("3"), Mode::Default).unwrap();
        let s = Shape::new(5, 2).unwrap();
        assert_eq!(r.assign(s, Rounding::Truncate).unwrap().to_string(), "0.66");
        assert_eq!(
            r.assign(s, Rounding::HalfAdjust).unwrap().to_string(),
            "0.67"
        );
        let r = mul(&lit("999"), &lit("999"), Mode::Default).unwrap();
        assert!(matches!(
            r.assign(s, Rounding::Truncate),
            Err(DecimalError::SizeOverflow { .. })
        ));
        assert_eq!(
            r.value
                .fit_truncate(s, Rounding::Truncate)
                .unwrap()
                .to_string(),
            "1.00"
        );
    }
}
