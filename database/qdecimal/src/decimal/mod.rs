// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The common decimal value: a sign, up to 63 digits and a scale.

pub mod digits;

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use digits::Digits;

use crate::error::{DecimalError, Result};

/// Most digits a value may have.
pub const MAX_DIGITS: u8 = 63;

/// How digits are dropped when a value is assigned to fewer decimals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rounding {
    /// Drop them.
    Truncate,
    /// Half-adjust: add one to the last kept digit when the first
    /// dropped digit is 5 or more, away from zero.
    HalfAdjust,
}

/// Digits and decimal positions of a field or an intermediate result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    /// Total digits, 1 to 63.
    pub digits: u8,
    /// Decimal positions, at most `digits`.
    pub scale: u8,
}

impl Shape {
    /// A validated shape.
    pub fn new(digits: u8, scale: u8) -> Result<Self> {
        let why = if digits == 0 {
            "no digits"
        } else if digits > MAX_DIGITS {
            "more than 63 digits"
        } else if scale > digits {
            "more decimal positions than digits"
        } else {
            return Ok(Self { digits, scale });
        };
        Err(DecimalError::Precision { digits, scale, why })
    }

    /// Digits before the decimal point.
    #[must_use]
    pub fn integer(self) -> u8 {
        self.digits - self.scale
    }
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{},{}", self.digits, self.scale)
    }
}

/// A decimal number: `(-1)^neg * mag / 10^scale`.
#[derive(Clone)]
pub struct Decimal {
    neg: bool,
    scale: u8,
    mag: Digits,
}

impl Decimal {
    /// Zero.
    #[must_use]
    pub fn zero() -> Self {
        Self {
            neg: false,
            scale: 0,
            mag: Vec::new(),
        }
    }

    /// `mantissa / 10^scale`.
    pub fn new(mantissa: i128, scale: u8) -> Result<Self> {
        Self::from_digits(
            mantissa < 0,
            digits::from_u128(mantissa.unsigned_abs()),
            scale,
        )
    }

    /// From little-endian digits.
    pub fn from_digits(neg: bool, mut mag: Digits, scale: u8) -> Result<Self> {
        digits::trim(&mut mag);
        if mag.iter().any(|d| *d > 9) {
            return Err(DecimalError::DecimalData {
                what: "digit above 9".to_string(),
            });
        }
        if scale > MAX_DIGITS {
            return Err(DecimalError::Precision {
                digits: MAX_DIGITS,
                scale,
                why: "more than 63 decimal positions",
            });
        }
        if mag.len() > usize::from(MAX_DIGITS) {
            return Err(DecimalError::SizeOverflow {
                value: Self::render(neg, &mag, scale),
                digits: MAX_DIGITS,
                scale,
            });
        }
        Ok(Self {
            neg: neg && !mag.is_empty(),
            scale,
            mag,
        })
    }

    /// Parse `[+-]digits[.digits]`.
    pub fn parse(text: &str) -> Result<Self> {
        let bad = || DecimalError::Parse {
            text: text.to_string(),
        };
        let s = text.trim();
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        if int.is_empty() && frac.is_empty() {
            return Err(bad());
        }
        let mut mag = Vec::with_capacity(int.len() + frac.len());
        for ch in frac.chars().rev().chain(int.chars().rev()) {
            let d = ch.to_digit(10).ok_or_else(bad)?;
            mag.push(u8::try_from(d).expect("digit"));
        }
        let scale = u8::try_from(frac.len()).map_err(|_| bad())?;
        Self::from_digits(neg, mag, scale)
    }

    /// Decimal positions.
    #[must_use]
    pub fn scale(&self) -> u8 {
        self.scale
    }

    /// Below zero.
    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.neg
    }

    /// Exactly zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// Little-endian digits of the unscaled magnitude, no high zeros.
    #[must_use]
    pub fn digits_le(&self) -> &[u8] {
        &self.mag
    }

    /// Digits a field needs to hold this value at its scale, at least 1.
    #[must_use]
    pub fn digits_needed(&self) -> u8 {
        let len = u8::try_from(self.mag.len()).expect("at most 63");
        len.max(self.scale).max(1)
    }

    /// Digits before the decimal point.
    #[must_use]
    pub fn integer_digits(&self) -> u8 {
        u8::try_from(self.mag.len())
            .expect("at most 63")
            .saturating_sub(self.scale)
    }

    /// The smallest shape holding this value.
    #[must_use]
    pub fn shape(&self) -> Shape {
        Shape {
            digits: self.digits_needed(),
            scale: self.scale,
        }
    }

    /// The unscaled integer, when it fits a machine word.
    #[must_use]
    pub fn mantissa_i128(&self) -> Option<i128> {
        let m = i128::try_from(digits::to_u128(&self.mag)?).ok()?;
        Some(if self.neg { -m } else { m })
    }

    /// The integer part, toward zero.
    #[must_use]
    pub fn to_i128(&self) -> Option<i128> {
        let (int, _) = digits::shr(&self.mag, usize::from(self.scale));
        let m = i128::try_from(digits::to_u128(&int)?).ok()?;
        Some(if self.neg { -m } else { m })
    }

    /// The same value with the sign flipped.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            neg: !self.neg && !self.mag.is_empty(),
            scale: self.scale,
            mag: self.mag.clone(),
        }
    }

    /// The magnitude.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self {
            neg: false,
            scale: self.scale,
            mag: self.mag.clone(),
        }
    }

    /// The value at another scale: zeros added, or digits dropped with
    /// `rounding`. Fails when the padded value needs more than 63 digits.
    pub fn rescale(&self, scale: u8, rounding: Rounding) -> Result<Self> {
        if scale >= self.scale {
            let mag = digits::shl(&self.mag, usize::from(scale - self.scale));
            return Self::from_digits(self.neg, mag, scale);
        }
        let drop = usize::from(self.scale - scale);
        let (mut kept, dropped) = digits::shr(&self.mag, drop);
        if rounding == Rounding::HalfAdjust && dropped.last().is_some_and(|d| *d >= 5) {
            kept = digits::add_one(&kept);
        }
        Self::from_digits(self.neg, kept, scale)
    }

    /// The value as a field of `shape` would hold it. Size error when the
    /// integer part does not fit.
    pub fn fit(&self, shape: Shape, rounding: Rounding) -> Result<Self> {
        let v = self.rescale(shape.scale, rounding)?;
        if v.integer_digits() > shape.integer() {
            return Err(DecimalError::SizeOverflow {
                value: self.to_string(),
                digits: shape.digits,
                scale: shape.scale,
            });
        }
        Ok(v)
    }

    /// Like [`fit`](Self::fit) but high-order digits that do not fit are
    /// dropped, as fixed-form RPG operations do.
    pub fn fit_truncate(&self, shape: Shape, rounding: Rounding) -> Result<Self> {
        let v = self.rescale(shape.scale, rounding)?;
        let keep = usize::from(shape.digits);
        if v.mag.len() <= keep {
            return Ok(v);
        }
        let mut mag = v.mag[..keep].to_vec();
        digits::trim(&mut mag);
        Self::from_digits(v.neg, mag, shape.scale)
    }

    /// Both magnitudes at the larger scale.
    fn aligned(&self, other: &Self) -> (Digits, Digits, u8) {
        let scale = self.scale.max(other.scale);
        let a = digits::shl(&self.mag, usize::from(scale - self.scale));
        let b = digits::shl(&other.mag, usize::from(scale - other.scale));
        (a, b, scale)
    }

    /// The exact sum as sign, magnitude and scale, however many digits
    /// it takes.
    pub(crate) fn sum_digits(&self, other: &Self) -> (bool, Digits, u8) {
        let (a, b, scale) = self.aligned(other);
        if self.neg == other.neg {
            return (self.neg, digits::add(&a, &b), scale);
        }
        match digits::cmp(&a, &b) {
            Ordering::Less => (other.neg, digits::sub(&b, &a), scale),
            _ => (self.neg, digits::sub(&a, &b), scale),
        }
    }

    /// Exact sum. Fails past 63 digits.
    pub fn add_exact(&self, other: &Self) -> Result<Self> {
        let (neg, mag, scale) = self.sum_digits(other);
        Self::from_digits(neg, mag, scale)
    }

    /// Exact difference. Fails past 63 digits.
    pub fn sub_exact(&self, other: &Self) -> Result<Self> {
        self.add_exact(&other.negated())
    }

    /// Exact product. Fails past 63 digits.
    pub fn mul_exact(&self, other: &Self) -> Result<Self> {
        let scale = self
            .scale
            .checked_add(other.scale)
            .filter(|s| *s <= MAX_DIGITS);
        let Some(scale) = scale else {
            return Err(DecimalError::Precision {
                digits: MAX_DIGITS,
                scale: self.scale.saturating_add(other.scale),
                why: "more than 63 decimal positions",
            });
        };
        Self::from_digits(
            self.neg != other.neg,
            digits::mul(&self.mag, &other.mag),
            scale,
        )
    }

    /// Quotient truncated to `scale` decimal positions.
    pub fn div_to_scale(&self, other: &Self, scale: u8) -> Result<Self> {
        if other.is_zero() {
            return Err(DecimalError::DivideByZero);
        }
        // a/b = (A * 10^(sb + scale)) / (B * 10^sa), truncated.
        let num = digits::shl(&self.mag, usize::from(other.scale) + usize::from(scale));
        let den = digits::shl(&other.mag, usize::from(self.scale));
        let (q, _) = digits::divmod(&num, &den);
        Self::from_digits(self.neg != other.neg, q, scale)
    }

    fn render(neg: bool, mag: &[u8], scale: u8) -> String {
        let mut s = String::new();
        if neg {
            s.push('-');
        }
        let scale = usize::from(scale);
        let int_len = mag.len().saturating_sub(scale);
        if int_len == 0 {
            s.push('0');
        } else {
            for d in mag[scale..].iter().rev() {
                s.push(char::from(b'0' + d));
            }
        }
        if scale > 0 {
            s.push('.');
            for i in (0..scale).rev() {
                s.push(char::from(b'0' + mag.get(i).copied().unwrap_or(0)));
            }
        }
        s
    }
}

impl Decimal {
    /// `%CHAR` of the value held in a field of `scale` decimals, as the
    /// machine renders it: every declared decimal shown, no zero before
    /// the point, the sign first.
    #[must_use]
    pub fn char_text(&self, scale: u8) -> String {
        let v = self
            .rescale(scale, Rounding::Truncate)
            .unwrap_or_else(|_| self.clone());
        let s = Self::render(v.neg, &v.mag, v.scale);
        if let Some(r) = s.strip_prefix("0.") {
            format!(".{r}")
        } else if let Some(r) = s.strip_prefix("-0.") {
            format!("-.{r}")
        } else {
            s
        }
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&Self::render(self.neg, &self.mag, self.scale))
    }
}

impl fmt::Debug for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Decimal({self})")
    }
}

impl FromStr for Decimal {
    type Err = DecimalError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.neg, other.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (neg, _) => {
                let (a, b, _) = self.aligned(other);
                let by_mag = digits::cmp(&a, &b);
                if neg { by_mag.reverse() } else { by_mag }
            }
        }
    }
}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Equal values hash alike whatever their scale.
        let mut mag = self.mag.clone();
        let mut scale = self.scale;
        while scale > 0 && mag.first() == Some(&0) {
            mag.remove(0);
            scale -= 1;
        }
        self.neg.hash(state);
        scale.hash(state);
        mag.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn parse_and_display() {
        assert_eq!(d("123.45").to_string(), "123.45");
        assert_eq!(d("-0.05").to_string(), "-0.05");
        assert_eq!(d("+7").to_string(), "7");
        assert_eq!(d(".5").to_string(), "0.5");
        assert_eq!(d("5.").to_string(), "5");
        assert_eq!(d("-0").to_string(), "0");
        assert_eq!(d("000.100").to_string(), "0.100");
        assert!(Decimal::parse("").is_err());
        assert!(Decimal::parse("1.2.3").is_err());
        assert!(Decimal::parse("12a").is_err());
        assert_eq!(Decimal::new(-12345, 2).unwrap().to_string(), "-123.45");
        assert_eq!(Decimal::new(5, 3).unwrap().to_string(), "0.005");
        let sixty_three = "9".repeat(63);
        assert_eq!(d(&sixty_three).digits_needed(), 63);
        assert!(Decimal::parse(&"1".repeat(64)).is_err());
    }

    #[test]
    fn compare_and_shape() {
        assert_eq!(d("1.0"), d("1.000"));
        assert!(d("-1") < d("0"));
        assert!(d("-2") < d("-1.5"));
        assert!(d("10") > d("9.99"));
        assert_eq!(d("0.05").digits_needed(), 2);
        assert_eq!(d("0.05").integer_digits(), 0);
        assert_eq!(
            d("123.45").shape(),
            Shape {
                digits: 5,
                scale: 2
            }
        );
        assert_eq!(d("0").digits_needed(), 1);
        assert_eq!(d("-123.45").mantissa_i128(), Some(-12345));
        assert_eq!(d("-123.45").to_i128(), Some(-123));
        assert_eq!(d("0.9").to_i128(), Some(0));
    }

    #[test]
    fn rescale_and_fit() {
        assert_eq!(
            d("1.25")
                .rescale(4, Rounding::Truncate)
                .unwrap()
                .to_string(),
            "1.2500"
        );
        assert_eq!(
            d("1.25")
                .rescale(1, Rounding::Truncate)
                .unwrap()
                .to_string(),
            "1.2"
        );
        assert_eq!(
            d("1.25")
                .rescale(1, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "1.3"
        );
        assert_eq!(
            d("-1.25")
                .rescale(1, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "-1.3"
        );
        assert_eq!(
            d("1.249")
                .rescale(1, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "1.2"
        );
        assert_eq!(
            d("9.99")
                .rescale(0, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "10"
        );
        assert_eq!(
            d("0.4")
                .rescale(0, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "0"
        );
        let s52 = Shape::new(5, 2).unwrap();
        assert_eq!(
            d("123.456")
                .fit(s52, Rounding::HalfAdjust)
                .unwrap()
                .to_string(),
            "123.46"
        );
        assert!(matches!(
            d("1234.5").fit(s52, Rounding::Truncate),
            Err(DecimalError::SizeOverflow {
                digits: 5,
                scale: 2,
                ..
            })
        ));
        assert_eq!(
            d("1234.5")
                .fit_truncate(s52, Rounding::Truncate)
                .unwrap()
                .to_string(),
            "234.50"
        );
        assert_eq!(
            d("999.995")
                .fit(s52, Rounding::HalfAdjust)
                .map(|v| v.to_string()),
            Err(DecimalError::SizeOverflow {
                value: "999.995".into(),
                digits: 5,
                scale: 2
            })
        );
        assert!(Shape::new(0, 0).is_err());
        assert!(Shape::new(64, 0).is_err());
        assert!(Shape::new(5, 6).is_err());
    }

    #[test]
    fn exact_arithmetic() {
        assert_eq!(d("1.5").add_exact(&d("2.25")).unwrap().to_string(), "3.75");
        assert_eq!(
            d("1.5").add_exact(&d("-2.25")).unwrap().to_string(),
            "-0.75"
        );
        assert_eq!(
            d("-1.5").add_exact(&d("-2.25")).unwrap().to_string(),
            "-3.75"
        );
        assert_eq!(d("2").sub_exact(&d("2.00")).unwrap().to_string(), "0.00");
        assert_eq!(
            d("1.5").mul_exact(&d("-2.25")).unwrap().to_string(),
            "-3.375"
        );
        assert_eq!(
            d("1").div_to_scale(&d("3"), 5).unwrap().to_string(),
            "0.33333"
        );
        assert_eq!(d("-7").div_to_scale(&d("2"), 0).unwrap().to_string(), "-3");
        assert_eq!(
            d("22.5").div_to_scale(&d("0.5"), 2).unwrap().to_string(),
            "45.00"
        );
        assert_eq!(
            d("1").div_to_scale(&d("0"), 2),
            Err(DecimalError::DivideByZero)
        );
        let big = d(&"9".repeat(63));
        assert!(big.add_exact(&d("1")).is_err());
        assert!(big.mul_exact(&d("10")).is_err());
    }
}
