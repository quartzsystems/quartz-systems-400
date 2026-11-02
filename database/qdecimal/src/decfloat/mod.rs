// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Decimal floating point: SQL's `DECFLOAT(16)` and `DECFLOAT(34)`, the
//! IEEE 754 decimal64 and decimal128 formats, with the special values
//! and the rounding (half-even) IBM i uses by default. `docs/DECIMAL.md`
//! §9 is the specification.

mod arith;
mod dpd;

use std::cmp::Ordering;
use std::fmt;

use crate::decimal::Decimal;
use crate::decimal::digits::{self, Digits};
use crate::error::{DecimalError, Result};

/// The two formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DfFormat {
    /// `DECFLOAT(16)`: decimal64.
    D16,
    /// `DECFLOAT(34)`: decimal128.
    D34,
}

impl DfFormat {
    /// The format of `DECFLOAT(n)`: 16 or 34.
    #[must_use]
    pub fn of_digits(n: u32) -> Option<Self> {
        match n {
            16 => Some(Self::D16),
            34 => Some(Self::D34),
            _ => None,
        }
    }

    /// Coefficient digits.
    #[must_use]
    pub fn digits(self) -> u32 {
        match self {
            Self::D16 => 16,
            Self::D34 => 34,
        }
    }

    /// Bytes stored.
    #[must_use]
    pub fn byte_len(self) -> usize {
        match self {
            Self::D16 => 8,
            Self::D34 => 16,
        }
    }

    /// The largest adjusted exponent.
    #[must_use]
    pub fn emax(self) -> i32 {
        match self {
            Self::D16 => 384,
            Self::D34 => 6144,
        }
    }

    /// The exponent bias of the encoding.
    fn bias(self) -> i32 {
        match self {
            Self::D16 => 398,
            Self::D34 => 6176,
        }
    }

    /// The largest exponent of the coefficient's last digit.
    fn qmax(self) -> i32 {
        self.emax() - self.p() + 1
    }

    /// The smallest exponent (subnormals included).
    fn etiny(self) -> i32 {
        1 - self.emax() - self.p() + 1
    }

    fn p(self) -> i32 {
        i32::try_from(self.digits()).expect("small")
    }
}

/// What a value is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DfKind {
    /// A number.
    Finite,
    /// Infinity.
    Infinity,
    /// A quiet NaN.
    NaN,
    /// A signaling NaN.
    SNaN,
}

/// A decimal floating point value: `(-1)^neg * coeff * 10^exp`, or a
/// special value. Trailing zeros are kept: `1.0` and `1.00` are equal
/// but print differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DecFloat {
    neg: bool,
    kind: DfKind,
    coeff: u128,
    exp: i32,
    format: DfFormat,
}

fn invalid(what: &'static str) -> DecimalError {
    DecimalError::Invalid { what }
}

impl DecFloat {
    /// Zero of a format.
    #[must_use]
    pub fn zero(format: DfFormat) -> Self {
        Self { neg: false, kind: DfKind::Finite, coeff: 0, exp: 0, format }
    }

    /// A special value.
    #[must_use]
    pub fn special(kind: DfKind, neg: bool, format: DfFormat) -> Self {
        Self { neg, kind, coeff: 0, exp: 0, format }
    }

    /// `coeff * 10^exp`, rounded to the format.
    pub fn new(neg: bool, coeff: u128, exp: i32, format: DfFormat) -> Result<Self> {
        Self::round(neg, &digits::from_u128(coeff), exp, format)
    }

    /// The format.
    #[must_use]
    pub fn format(&self) -> DfFormat {
        self.format
    }

    /// What it is.
    #[must_use]
    pub fn kind(&self) -> DfKind {
        self.kind
    }

    /// The sign bit.
    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.neg
    }

    /// A number, not a special value.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.kind == DfKind::Finite
    }

    /// Quiet or signaling NaN.
    #[must_use]
    pub fn is_nan(&self) -> bool {
        matches!(self.kind, DfKind::NaN | DfKind::SNaN)
    }

    /// A finite zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.is_finite() && self.coeff == 0
    }

    /// The coefficient.
    #[must_use]
    pub fn coefficient(&self) -> u128 {
        self.coeff
    }

    /// The exponent.
    #[must_use]
    pub fn exponent(&self) -> i32 {
        self.exp
    }

    /// The same value in another format, rounded when narrower.
    pub fn to_format(&self, format: DfFormat) -> Result<Self> {
        if !self.is_finite() {
            return Ok(Self { format, ..*self });
        }
        Self::round(self.neg, &digits::from_u128(self.coeff), self.exp, format)
    }

    /// Round `mag * 10^exp` to the format, half-even; clamp the exponent.
    pub(crate) fn round(neg: bool, mag: &[u8], exp: i32, format: DfFormat) -> Result<Self> {
        let mut mag: Digits = mag.to_vec();
        digits::trim(&mut mag);
        let mut exp = exp;
        let p = format.digits() as usize;
        // Too many digits, or below the smallest exponent: drop some.
        let len = i32::try_from(mag.len()).expect("small");
        let mut drop = (len - format.p()).max(0);
        if exp + drop < format.etiny() {
            drop = format.etiny() - exp;
        }
        if drop > 0 {
            let n = usize::try_from(drop).expect("positive");
            let (kept, dropped) = digits::shr(&mag, n);
            mag = kept;
            exp += drop;
            if round_up(&mag, &dropped) {
                mag = digits::add_one(&mag);
                if mag.len() > p {
                    mag = digits::shr(&mag, 1).0;
                    exp += 1;
                }
            }
        }
        if mag.is_empty() {
            exp = exp.clamp(format.etiny(), format.qmax());
        } else if exp > format.qmax() {
            // Fold down: pad with zeros when the digits allow.
            let pad = usize::try_from(exp - format.qmax()).expect("positive");
            if mag.len() + pad > p {
                return Err(DecimalError::Overflow { what: "a DECFLOAT result exceeds its largest exponent" });
            }
            mag = digits::shl(&mag, pad);
            exp = format.qmax();
        }
        let coeff = digits::to_u128(&mag).expect("at most 34 digits");
        Ok(Self { neg, kind: DfKind::Finite, coeff, exp, format })
    }

    /// From a decimal, rounded when it has more digits than the format.
    pub fn from_decimal(d: &Decimal, format: DfFormat) -> Result<Self> {
        Self::round(d.is_negative(), d.digits_le(), -i32::from(d.scale()), format)
    }

    /// From an integer.
    #[must_use]
    pub fn from_i64(n: i64, format: DfFormat) -> Self {
        Self::round(n < 0, &digits::from_u128(u128::from(n.unsigned_abs())), 0, format).expect("fits")
    }

    /// The exact decimal, when finite and within 63 digits and 63
    /// decimal positions.
    pub fn to_decimal(&self) -> Result<Decimal> {
        match self.kind {
            DfKind::Finite => {}
            DfKind::Infinity => return Err(DecimalError::NotFinite { what: "infinity" }),
            DfKind::NaN | DfKind::SNaN => return Err(DecimalError::NotFinite { what: "NaN" }),
        }
        let mag = digits::from_u128(self.coeff);
        if self.exp >= 0 {
            let e = usize::try_from(self.exp).expect("positive");
            if mag.len() + e > usize::from(crate::MAX_DIGITS) && !mag.is_empty() {
                return Err(DecimalError::SizeOverflow { value: self.to_string(), digits: 63, scale: 0 });
            }
            return Decimal::from_digits(self.neg, digits::shl(&mag, e), 0);
        }
        // Drop trailing zeros past 63 decimal positions; others are lost.
        let mut mag = mag;
        let mut scale = -self.exp;
        while scale > 63 && mag.first() == Some(&0) {
            mag.remove(0);
            scale -= 1;
        }
        if scale > 63 {
            return Err(DecimalError::SizeOverflow { value: self.to_string(), digits: 63, scale: 63 });
        }
        Decimal::from_digits(self.neg, mag, u8::try_from(scale).expect("at most 63"))
    }

    /// The nearest double.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        let sign = if self.neg { -1.0 } else { 1.0 };
        match self.kind {
            DfKind::Infinity => sign * f64::INFINITY,
            DfKind::NaN | DfKind::SNaN => f64::NAN,
            DfKind::Finite => format!("{}{}E{}", if self.neg { "-" } else { "" }, self.coeff, self.exp)
                .parse()
                .unwrap_or(0.0),
        }
    }

    /// From a double: its shortest text.
    pub fn from_f64(f: f64, format: DfFormat) -> Result<Self> {
        if f.is_nan() {
            return Ok(Self::special(DfKind::NaN, false, format));
        }
        if f.is_infinite() {
            return Ok(Self::special(DfKind::Infinity, f < 0.0, format));
        }
        Self::parse(&format!("{f:e}"), format)
    }

    /// Parse `[+-]digits[.digits][E[+-]n]`, `Infinity`, `Inf`, `NaN` or
    /// `sNaN` (any case), rounded to the format.
    pub fn parse(text: &str, format: DfFormat) -> Result<Self> {
        let bad = || DecimalError::Parse { text: text.to_string() };
        let s = text.trim();
        let (neg, s) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        match s.to_ascii_uppercase().as_str() {
            "INF" | "INFINITY" => return Ok(Self::special(DfKind::Infinity, neg, format)),
            "NAN" => return Ok(Self::special(DfKind::NaN, neg, format)),
            "SNAN" => return Ok(Self::special(DfKind::SNaN, neg, format)),
            _ => {}
        }
        let (num, e) = match s.find(['E', 'e']) {
            Some(i) => (&s[..i], s[i + 1..].parse::<i64>().map_err(|_| bad())?),
            None => (s, 0),
        };
        let (int, frac) = num.split_once('.').unwrap_or((num, ""));
        if int.is_empty() && frac.is_empty() {
            return Err(bad());
        }
        let mut mag = Vec::with_capacity(int.len() + frac.len());
        for ch in frac.chars().rev().chain(int.chars().rev()) {
            mag.push(u8::try_from(ch.to_digit(10).ok_or_else(bad)?).expect("digit"));
        }
        let exp = e - i64::try_from(frac.len()).map_err(|_| bad())?;
        // Far past any format: zero clamps, anything else overflows or underflows.
        let exp = i32::try_from(exp.clamp(-100_000, 100_000)).expect("clamped");
        Self::round(neg, &mag, exp, format)
    }

    /// Negated (the sign of a NaN too).
    #[must_use]
    pub fn negated(&self) -> Self {
        Self { neg: !self.neg, ..*self }
    }

    /// Without its sign.
    #[must_use]
    pub fn abs(&self) -> Self {
        Self { neg: false, ..*self }
    }

    /// Trailing zeros removed (`NORMALIZE_DECFLOAT`); zero is `0`.
    #[must_use]
    pub fn normalized(&self) -> Self {
        if !self.is_finite() {
            return *self;
        }
        if self.coeff == 0 {
            return Self { exp: 0, ..*self };
        }
        let (mut c, mut e) = (self.coeff, self.exp);
        while c % 10 == 0 && e < self.format.qmax() {
            c /= 10;
            e += 1;
        }
        Self { coeff: c, exp: e, ..*self }
    }

    /// The IEEE encoding, big-endian, densely packed decimal.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        dpd::encode(self)
    }

    /// From the encoding.
    pub fn decode(bytes: &[u8], format: DfFormat) -> Result<Self> {
        if bytes.len() != format.byte_len() {
            return Err(DecimalError::DecimalData {
                what: format!("{} bytes where DECFLOAT({}) has {}", bytes.len(), format.digits(), format.byte_len()),
            });
        }
        Ok(dpd::decode(bytes, format))
    }

    /// Order as SQL compares: `-NaN < -sNaN < -Infinity < numbers <
    /// Infinity < sNaN < NaN`; numbers by value, so `1.0 = 1.00`.
    #[must_use]
    pub fn sql_cmp(&self, other: &Self) -> Ordering {
        let rank = |v: &Self| -> i8 {
            let r = match v.kind {
                DfKind::Finite => 0,
                DfKind::Infinity => 1,
                DfKind::SNaN => 2,
                DfKind::NaN => 3,
            };
            if v.neg { -r } else { r }
        };
        let (ra, rb) = (rank(self), rank(other));
        if ra != 0 || rb != 0 {
            return ra.cmp(&rb);
        }
        cmp_finite(self, other)
    }

    /// Order of the IEEE total order (`TOTALORDER`): as [`Self::sql_cmp`]
    /// but `-0 < 0` and among equal values the smaller exponent first.
    #[must_use]
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        let o = self.sql_cmp(other);
        if o != Ordering::Equal || !self.is_finite() {
            return o;
        }
        let by_exp = self.exp.cmp(&other.exp);
        match (self.neg, other.neg) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => by_exp.reverse(),
            (false, false) => by_exp,
        }
    }
}

/// Whether the dropped digits (high first at the end) round `kept` up,
/// half-even.
fn round_up(kept: &[u8], dropped: &[u8]) -> bool {
    let Some((&first, rest)) = dropped.split_last() else { return false };
    match first.cmp(&5) {
        Ordering::Less => false,
        Ordering::Greater => true,
        Ordering::Equal => rest.iter().any(|d| *d != 0) || kept.first().is_some_and(|d| d % 2 == 1),
    }
}

/// Finite values by value.
fn cmp_finite(a: &DecFloat, b: &DecFloat) -> Ordering {
    let (za, zb) = (a.coeff == 0, b.coeff == 0);
    let sign = |neg: bool, zero: bool| -> i8 {
        if zero {
            0
        } else if neg {
            -1
        } else {
            1
        }
    };
    let (sa, sb) = (sign(a.neg, za), sign(b.neg, zb));
    if sa != sb || sa == 0 {
        return sa.cmp(&sb);
    }
    let mag = cmp_magnitude(a, b);
    if sa < 0 { mag.reverse() } else { mag }
}

/// Nonzero magnitudes.
fn cmp_magnitude(a: &DecFloat, b: &DecFloat) -> Ordering {
    let (ma, mb) = (digits::from_u128(a.coeff), digits::from_u128(b.coeff));
    let adj = |m: &Digits, e: i32| e + i32::try_from(m.len()).expect("small");
    let (aa, ab) = (adj(&ma, a.exp), adj(&mb, b.exp));
    if aa != ab {
        return aa.cmp(&ab);
    }
    let e = a.exp.min(b.exp);
    let wa = digits::shl(&ma, usize::try_from(a.exp - e).expect("small"));
    let wb = digits::shl(&mb, usize::try_from(b.exp - e).expect("small"));
    digits::cmp(&wa, &wb)
}

impl fmt::Display for DecFloat {
    /// Scientific text as IEEE 754 and SQL give it: plain when the
    /// exponent is at most 0 and the adjusted exponent at least -6,
    /// else `d.dddE+n`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.neg {
            f.write_str("-")?;
        }
        match self.kind {
            DfKind::Infinity => return f.write_str("Infinity"),
            DfKind::NaN => return f.write_str("NaN"),
            DfKind::SNaN => return f.write_str("SNaN"),
            DfKind::Finite => {}
        }
        let c = self.coeff.to_string();
        let len = i32::try_from(c.len()).expect("small");
        let adj = self.exp + len - 1;
        if self.exp <= 0 && adj >= -6 {
            if self.exp == 0 {
                return f.write_str(&c);
            }
            let point = len + self.exp;
            if point > 0 {
                let at = usize::try_from(point).expect("positive");
                return write!(f, "{}.{}", &c[..at], &c[at..]);
            }
            let zeros = usize::try_from(-point).expect("positive");
            return write!(f, "0.{}{c}", "0".repeat(zeros));
        }
        let (head, tail) = c.split_at(1);
        f.write_str(head)?;
        if !tail.is_empty() {
            write!(f, ".{tail}")?;
        }
        write!(f, "E{}{adj}", if adj >= 0 { "+" } else { "" })
    }
}

pub use arith::{add, div, mul, sub};

#[cfg(test)]
mod tests;
