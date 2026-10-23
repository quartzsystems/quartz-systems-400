// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arithmetic with the intermediate precision rules of the RPG runtime.
//! Operands carry a declared shape, as fields and literals do; the
//! result's shape follows the rules in `docs/DECIMAL.md`, fitted to the
//! pub400 capture, and the value is truncated to it.

use crate::decimal::{Decimal, MAX_DIGITS, Rounding, Shape, digits};
use crate::error::{DecimalError, Result};

/// Where an operation's result goes and the options on the way. The
/// default is an expression with no numeric result field, such as an
/// argument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mode {
    /// Decimal positions of the field the result is assigned to; 0 for
    /// an argument.
    pub result_scale: u8,
    /// `EXPROPTS(*RESDECPOS)` or `EVAL(R)`: the result's decimals are
    /// kept even where integer digits must give way.
    pub resdecpos: bool,
    /// `EVAL(H)` or the `H` extender: one decimal more than the result
    /// is kept, for the half-adjust at assignment.
    pub half_adjust: bool,
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

/// The digits an intermediate result may use before its decimals give
/// way: 30, a 31-digit operand's 31, and 63 once an operand is wider
/// than 31. Fitted to the pub400 capture, which has no operand of 32 to
/// 39 digits.
fn limit(a: Shape, b: Shape) -> u16 {
    let widest = u16::from(a.digits.max(b.digits));
    if widest > 31 { 63 } else { widest.max(30) }
}

/// The shape of an intermediate result from its natural `integer`
/// digits and `scale` decimals. Decimals give way down to `limit`, but
/// not below the result's own (one more under half-adjust). Past 63
/// digits the result's decimals go too, unless `*RESDECPOS` holds them
/// and integer digits give way instead; a value that then does not fit
/// is a size error when the result settles.
fn shape(integer: u16, scale: u16, limit: u16, mode: Mode, value: &Decimal) -> Result<Shape> {
    let max = u16::from(MAX_DIGITS);
    let floor = u16::from(mode.result_scale) + u16::from(mode.half_adjust);
    let mut integer = integer;
    let mut scale = scale;
    if integer + scale > limit {
        scale = scale.min(limit.saturating_sub(integer).max(floor));
    }
    if integer + scale > max {
        if !mode.resdecpos {
            scale = max.saturating_sub(integer);
        }
        if scale > max {
            return Err(DecimalError::SizeOverflow {
                value: value.to_string(),
                digits: MAX_DIGITS,
                scale: MAX_DIGITS,
            });
        }
        integer = max - scale;
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
    // The exact sum may carry more digits than any Decimal holds: add
    // the digit vectors and drop the excess decimals at once.
    let (neg, mag, at) = a.value.sum_digits(&b.value);
    let shape = shape(integer, scale, limit(a.shape, b.shape), mode, &a.value)?;
    let (kept, _) = digits::shr(&mag, usize::from(at.saturating_sub(shape.scale)));
    let exact = Decimal::from_digits(neg, kept, at.min(shape.scale))?;
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
    let shape = shape(integer, scale, limit(a.shape, b.shape), mode, &a.value)?;
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

/// `a / b`: `i1 + d2` integer digits and every position the limit
/// leaves as decimals.
pub fn div(a: &Typed, b: &Typed, mode: Mode) -> Result<Typed> {
    if b.value.is_zero() {
        return Err(DecimalError::DivideByZero);
    }
    let integer = u16::from(a.shape.integer()) + u16::from(b.shape.scale);
    let limit = limit(a.shape, b.shape);
    let shape = shape(integer, limit, limit, mode, &a.value)?;
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

    fn shape_of(r: &Typed) -> (u8, u8) {
        (r.shape.digits, r.shape.scale)
    }

    #[test]
    fn shapes_follow_the_rules() {
        let m = Mode::default();
        let r = add(&t("123.45", 5, 2), &t("1.234", 7, 3), m).unwrap();
        assert_eq!(shape_of(&r), (8, 3));
        assert_eq!(r.value.to_string(), "124.684");
        let r = sub(&t("1", 1, 0), &t("2", 1, 0), m).unwrap();
        assert_eq!(shape_of(&r), (2, 0));
        assert_eq!(r.value.to_string(), "-1");
        let r = mul(&t("12.5", 3, 1), &t("-2.25", 3, 2), m).unwrap();
        assert_eq!(shape_of(&r), (6, 3));
        assert_eq!(r.value.to_string(), "-28.125");
        // A quotient of small operands fills 30 digits (pub400: 1 / 3 in
        // 1,0 fields shows 29 threes).
        let r = div(&t("1", 1, 0), &t("3", 1, 0), m).unwrap();
        assert_eq!(shape_of(&r), (30, 29));
        assert_eq!(r.value.to_string(), format!("0.{}", "3".repeat(29)));
        let r = div(&t("100.0", 5, 1), &t("0.25", 3, 2), m).unwrap();
        assert_eq!(r.shape.integer(), 6);
        assert_eq!(r.shape.scale, 24);
        assert_eq!(r.value.to_i128(), Some(400));
        assert_eq!(
            div(&lit("1"), &lit("0"), m),
            Err(DecimalError::DivideByZero)
        );
    }

    #[test]
    fn thirty_one_and_sixty_three_digit_limits() {
        let m = Mode::default();
        // A 31-digit operand widens the limit to 31 (pub400: 999.99 /
        // 999999999999999999999.9999999999 has 18 decimals, 5,2 + 31,10
        // has 9).
        let r = div(&t("999.99", 5, 2), &t("3", 31, 10), m).unwrap();
        assert_eq!(shape_of(&r), (31, 18));
        let r = add(&t("1.5", 5, 2), &t("1", 31, 10), m).unwrap();
        assert_eq!(shape_of(&r), (31, 9));
        // Integer digits never give way by default: 31,10 * 31,10 keeps
        // its 42 integer digits and no decimals (pub400 case 767).
        let big = t(&format!("{}.9999999999", "9".repeat(21)), 31, 10);
        let r = mul(&big, &big, m).unwrap();
        assert_eq!(shape_of(&r), (42, 0));
        assert_eq!(
            r.value.to_string(),
            "999999999999999999999999999999800000000000"
        );
        // Wider operands compute in 63 digits; decimals give way first.
        let a = t("1.5", 40, 20);
        let b = t("2.5", 40, 20);
        let r = mul(&a, &b, m).unwrap();
        assert_eq!(shape_of(&r), (63, 23));
        assert_eq!(r.value.to_string(), format!("3.75{}", "0".repeat(21)));
        let r = mul(&t(&"9".repeat(31), 31, 0), &t(&"9".repeat(32), 32, 0), m).unwrap();
        assert_eq!(shape_of(&r), (63, 0));
        let big = t(&"1".repeat(40), 40, 0);
        assert!(matches!(
            mul(&big, &big, m),
            Err(DecimalError::SizeOverflow { .. })
        ));
        // A sum past 63 digits loses a decimal, not the operation
        // (pub400 case 148).
        let nines = t(&format!("{}.{}", "9".repeat(31), "9".repeat(32)), 63, 32);
        let r = add(&t("999.99", 5, 2), &nines, m).unwrap();
        assert_eq!(shape_of(&r), (63, 31));
        assert_eq!(
            r.value.to_string(),
            format!("1{}999.98{}", "0".repeat(28), "9".repeat(29))
        );
        let r = add(&t("1", 63, 0), &t("1", 63, 0), m).unwrap();
        assert_eq!(shape_of(&r), (63, 0));
        assert_eq!(r.value.to_string(), "2");
    }

    #[test]
    fn the_result_field_keeps_its_decimals() {
        // 31,10 / 31,10 leaves no decimals for an argument, but a 15,5
        // result keeps its 5, one more under half-adjust (pub400 case 775:
        // %CHAR(1 / 3) is 0, r1 = 1 / 3 is .33333).
        let a = t("1", 31, 10);
        let b = t("3", 31, 10);
        let r = div(&a, &b, Mode::default()).unwrap();
        assert_eq!(shape_of(&r), (31, 0));
        assert!(r.value.is_zero());
        let to_15_5 = Mode {
            result_scale: 5,
            ..Mode::default()
        };
        let r = div(&a, &b, to_15_5).unwrap();
        assert_eq!(shape_of(&r), (36, 5));
        assert_eq!(r.value.to_string(), "0.33333");
        let r = div(
            &t("-1", 31, 10),
            &t("7", 31, 10),
            Mode {
                half_adjust: true,
                ..to_15_5
            },
        )
        .unwrap();
        assert_eq!(shape_of(&r), (37, 6));
        let s = Shape::new(15, 5).unwrap();
        assert_eq!(
            r.assign(s, Rounding::HalfAdjust).unwrap().to_string(),
            "-0.14286"
        );
        // Past 63 digits the result's decimals go too, unless *RESDECPOS
        // holds them and integer digits give way (pub400 cases 1174 and
        // 1342: r1 is .00000, rr is .33333).
        let a = t("1", 45, 14);
        let b = t("3", 63, 32);
        let r = div(&a, &b, to_15_5).unwrap();
        assert_eq!(shape_of(&r), (63, 0));
        let r = div(
            &a,
            &b,
            Mode {
                resdecpos: true,
                ..to_15_5
            },
        )
        .unwrap();
        assert_eq!(shape_of(&r), (63, 5));
        assert_eq!(r.shape.integer(), 58);
        assert_eq!(r.value.to_string(), "0.33333");
        // *RESDECPOS fails when the value no longer fits.
        let wide = t(&"9".repeat(60), 63, 0);
        assert!(matches!(
            div(
                &wide,
                &t("1", 63, 0),
                Mode {
                    result_scale: 5,
                    resdecpos: true,
                    half_adjust: false,
                }
            ),
            Err(DecimalError::SizeOverflow { .. })
        ));
    }

    #[test]
    fn assignment_rounds_and_overflows() {
        let m = Mode::default();
        let r = div(&lit("2"), &lit("3"), m).unwrap();
        let s = Shape::new(5, 2).unwrap();
        assert_eq!(r.assign(s, Rounding::Truncate).unwrap().to_string(), "0.66");
        assert_eq!(
            r.assign(s, Rounding::HalfAdjust).unwrap().to_string(),
            "0.67"
        );
        let r = mul(&lit("999"), &lit("999"), m).unwrap();
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
