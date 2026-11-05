// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Character strings as numbers, the way a `CAST` reads them: blanks
//! around the number, an optional sign right before the digits, digits
//! with an optional point, an optional exponent. Anything else (inner
//! blanks, a trailing sign, `0x`, `Infinity` for a float, an empty
//! string) is `SQL0420`; a number past the target is `SQL0802`.

use qdecimal::Decimal;

use super::{bad_chars, out_of_range};
use crate::error::Result;

/// A number read from text: its sign, digits (high first), the power of
/// ten of the last digit.
#[derive(Debug, PartialEq, Eq)]
pub struct Parsed {
    /// Negative.
    pub neg: bool,
    /// Digits, most significant first, leading zeros removed.
    pub digits: Vec<u8>,
    /// The exponent of the last digit.
    pub exp: i64,
}

/// Read `text` as a number.
pub fn parse(text: &str) -> Result<Parsed> {
    let bad = || bad_chars(text);
    let t = text.trim_matches(' ');
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let (num, e) = match t.find(['E', 'e']) {
        Some(i) => {
            let e = &t[i + 1..];
            let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            (&t[..i], e.parse::<i64>().unwrap_or(if e.starts_with('-') { -100_000 } else { 100_000 }))
        }
        None => (t, 0),
    };
    let (int, frac) = num.split_once('.').unwrap_or((num, ""));
    if (int.is_empty() && frac.is_empty()) || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes()).map(|b| b - b'0').collect();
    let lead = digits.iter().take_while(|d| **d == 0).count();
    digits.drain(..lead);
    let exp = e.clamp(-100_000, 100_000) - i64::try_from(frac.len()).unwrap_or(i64::MAX);
    Ok(Parsed { neg, digits, exp })
}

impl Parsed {
    /// The exact decimal with at most `scale` decimal places (more are
    /// cut off); past 63 digits is out of range.
    pub fn decimal(&self, scale: u8, text: &str) -> Result<Decimal> {
        let mut digits = self.digits.clone();
        let mut exp = self.exp;
        // Drop the places below the scale.
        let keep_from = -i64::from(scale);
        if exp < keep_from {
            let drop = usize::try_from(keep_from - exp).unwrap_or(usize::MAX).min(digits.len());
            digits.truncate(digits.len() - drop);
            exp = keep_from;
        }
        if digits.is_empty() {
            return Ok(Decimal::zero());
        }
        if exp > 0 {
            if exp > 63 {
                return Err(out_of_range(text));
            }
            digits.extend(std::iter::repeat_n(0, usize::try_from(exp).expect("small")));
            exp = 0;
        }
        if digits.len() > 63 {
            return Err(out_of_range(text));
        }
        let scale = u8::try_from(-exp).expect("at most the scale");
        let mag: Vec<u8> = digits.into_iter().rev().collect();
        Decimal::from_digits(self.neg, mag, scale).map_err(|_| out_of_range(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(t: &str, scale: u8) -> String {
        parse(t).and_then(|p| p.decimal(scale, t)).map_or_else(|e| e.msgid(), |d| d.to_string())
    }

    #[test]
    fn the_primary_s_rules() {
        assert_eq!(dec(" 12 ", 0), "12");
        assert_eq!(dec("+12", 0), "12");
        assert_eq!(dec("-12", 0), "-12");
        assert_eq!(dec("12.", 1), "12");
        assert_eq!(dec(".5", 2), "0.5");
        assert_eq!(dec("12.7", 0), "12");
        assert_eq!(dec("1E2", 0), "100");
        assert_eq!(dec("1.239", 2), "1.23");
        assert_eq!(dec("1.0E1", 0), "10");
        for bad in ["1 2", "", "- 5", "5-", "0x1F", "Infinity", "NaN", "abc", "1E", "."] {
            assert_eq!(dec(bad, 0), "SQL0420", "{bad}");
        }
        assert_eq!(dec("1E400", 0), "SQL0802");
    }
}
