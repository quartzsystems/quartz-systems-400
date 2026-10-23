// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `%CHAR` of numbers: no leading zeros (not even before the decimal
//! point), a leading minus, every decimal position, the `DECEDIT`
//! point; floats as `d.dddddddddddddddE+ddd`.

use qdecimal::{Decimal, Rounding, Shape};

/// `%CHAR` of a decimal value held with `shape`'s decimals.
#[must_use]
pub fn char_of(v: &Decimal, shape: Shape, point: char) -> String {
    let v = v.rescale(shape.scale, Rounding::Truncate).unwrap_or_else(|_| v.clone());
    let s = v.to_string();
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b.to_string()),
        None => (false, s),
    };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (body, String::new()),
    };
    let int = int.trim_start_matches('0').to_string();
    let mut out = String::new();
    if neg && (int.chars().any(|c| c != '0') || frac.chars().any(|c| c != '0')) {
        out.push('-');
    }
    if shape.scale == 0 {
        out.push_str(if int.is_empty() { "0" } else { &int });
    } else {
        out.push_str(&int);
        out.push(point);
        let mut f = frac;
        while f.len() < usize::from(shape.scale) {
            f.push('0');
        }
        out.push_str(&f);
    }
    out
}

/// `%CHAR` of a float: the sign, one digit, the point, 15 decimals and
/// a three-digit exponent (8-byte), or 7 decimals and two (4-byte).
#[must_use]
pub fn char_of_float(f: f64, single: bool) -> String {
    let (decimals, width) = if single { (7, 2) } else { (15, 3) };
    if f == 0.0 {
        return format!("+0.{}E+{:0width$}", "0".repeat(decimals), 0);
    }
    let s = format!("{f:+.decimals$e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    let sign = if e < 0 { '-' } else { '+' };
    format!("{mant}E{sign}{:0width$}", e.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        Decimal::parse(s).unwrap()
    }

    #[test]
    fn numbers_as_ibm_prints_them() {
        let sh = |a, b| Shape::new(a, b).unwrap();
        assert_eq!(char_of(&d("12.5"), sh(7, 2), '.'), "12.50");
        assert_eq!(char_of(&d("0.5"), sh(7, 2), '.'), ".50");
        assert_eq!(char_of(&d("-0.5"), sh(7, 2), '.'), "-.50");
        assert_eq!(char_of(&d("0"), sh(7, 2), '.'), ".00");
        assert_eq!(char_of(&d("0"), sh(5, 0), '.'), "0");
        assert_eq!(char_of(&d("-42"), sh(5, 0), '.'), "-42");
        assert_eq!(char_of(&d("3.25"), sh(5, 2), ','), "3,25");
        assert_eq!(char_of_float(1.5, false), "+1.500000000000000E+000");
        assert_eq!(char_of_float(100.0, true), "+1.0000000E+02");
        assert_eq!(char_of_float(0.0, true), "+0.0000000E+00");
        assert_eq!(char_of_float(-0.000125, false), "-1.250000000000000E-004");
    }
}
