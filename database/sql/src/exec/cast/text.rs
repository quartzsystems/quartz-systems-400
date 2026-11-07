// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A value as IBM i spells it in a character string: what `CHAR`,
//! `VARCHAR`, `CAST(... AS CHAR)` and concatenation give.
//! `tools/conformance/sql/cases/types/FLOATFMT.sql` and `DECFLT1.sql`
//! hold the primary's forms.

use qdecimal::{DecFloat, DfKind};

use crate::value::Value;

/// A double in IBM's form: as many significant digits as its binary
/// value carries (17 for a normal double, fewer for a subnormal one),
/// trailing zeros dropped, one digit before the point and `E` with the
/// exponent: `1.5E0`, `1.0000000000000001E-1`, `1E15`, `4.9E-324`.
/// Zero is `0E0`, without a sign.
#[must_use]
pub fn float_char(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.into();
    }
    if x == 0.0 {
        return "0E0".into();
    }
    // Significant bits: 53, or a subnormal's own.
    let bits = x.to_bits();
    let raw_exp = (bits >> 52) & 0x7FF;
    let frac = bits & ((1 << 52) - 1);
    let sig = if raw_exp == 0 {
        64 - frac.leading_zeros()
    } else {
        53
    };
    // floor(sig * log10(2)) + 2: 17 for 53 bits, 2 for one.
    let digits = usize::try_from(u64::from(sig) * 30_103 / 100_000 + 2).expect("small");
    let s = format!("{:.*e}", digits - 1, x);
    let (mant, exp) = s.split_once('e').expect("exponent form");
    let mant = if mant.contains('.') {
        mant.trim_end_matches('0').trim_end_matches('.')
    } else {
        mant
    };
    format!("{mant}E{exp}")
}

/// A `DECFLOAT` in IBM's form: the scientific string with the special
/// values in capitals (`INFINITY`, `-NAN`, `SNAN`).
#[must_use]
pub fn decfloat_char(d: &DecFloat) -> String {
    let sign = if d.is_negative() { "-" } else { "" };
    match d.kind() {
        DfKind::Finite => d.to_string(),
        DfKind::Infinity => format!("{sign}INFINITY"),
        DfKind::NaN => format!("{sign}NAN"),
        DfKind::SNaN => format!("{sign}SNAN"),
    }
}

/// Any value as character text: numbers in IBM's forms (`.5`, `-12.50`,
/// `1.5E0`), `TRUE` and `FALSE`, the rest as it displays.
#[must_use]
pub fn char_text(v: &Value) -> String {
    match v {
        Value::Num(d) => d.char_text(d.scale()),
        Value::Double(x) => float_char(*x),
        Value::Real(x) => float_char(f64::from(*x)),
        Value::DecFloat(d) => decfloat_char(d),
        Value::Timestamp(t) => t.render_field(),
        v => v.as_text(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_as_the_primary_spells_them() {
        for (x, s) in [
            (1.5, "1.5E0"),
            (0.1, "1.0000000000000001E-1"),
            (1.0, "1E0"),
            (-2.5E-3, "-2.5000000000000001E-3"),
            (1E300, "1.0000000000000001E300"),
            (123_456_789.0, "1.23456789E8"),
            (1.0E15, "1E15"),
            (1.0E-5, "1.0000000000000001E-5"),
            (1E-7, "9.9999999999999995E-8"),
            (-0.0, "0E0"),
            (1.797_693_134_862_315_7E308, "1.7976931348623157E308"),
            (4.9E-324, "4.9E-324"),
            (2.225_073_858_507_201_4E-308, "2.2250738585072014E-308"),
            (12345.678, "1.2345678E4"),
            (9_007_199_254_740_993.0, "9.007199254740992E15"),
            (100.0, "1E2"),
        ] {
            assert_eq!(float_char(x), s, "{x}");
        }
        for (x, s) in [
            (0.1f32, "1.0000000149011612E-1"),
            (1.5, "1.5E0"),
            (3.402_823_5E38, "3.4028234663852886E38"),
            (-123.456, "-1.2345600128173828E2"),
            (1E-10, "1.000000013351432E-10"),
        ] {
            assert_eq!(char_text(&Value::Real(x)), s, "{x}");
        }
    }
}
