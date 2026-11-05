// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use std::cmp::Ordering;

use super::*;

fn d16(s: &str) -> DecFloat {
    DecFloat::parse(s, DfFormat::D16).unwrap()
}

fn d34(s: &str) -> DecFloat {
    DecFloat::parse(s, DfFormat::D34).unwrap()
}

#[test]
fn text_forms() {
    for (input, out) in [
        ("0", "0"),
        ("1.50", "1.50"),
        ("-12.5", "-12.5"),
        ("1.5E-30", "1.5E-30"),
        ("123E3", "1.23E+5"),
        ("0.000001", "0.000001"),
        ("0.0000001", "1E-7"),
        ("1E+3", "1E+3"),
        ("0E-2", "0.00"),
        ("inf", "Infinity"),
        ("-Infinity", "-Infinity"),
        ("NaN", "NaN"),
        ("snan", "SNaN"),
        ("-nan", "-NaN"),
    ] {
        assert_eq!(d34(input).to_string(), out, "{input}");
    }
    assert!(DecFloat::parse("1.2.3", DfFormat::D16).is_err());
    assert!(DecFloat::parse("", DfFormat::D16).is_err());
    assert!(DecFloat::parse("E5", DfFormat::D16).is_err());
}

#[test]
fn exponents_kept() {
    // (text, coefficient, exponent, text back): equal values that
    // differ in exponent stay apart through text and the encodings.
    let cases: [(&str, u128, i32, &str); 15] = [
        ("1.5", 15, -1, "1.5"),
        ("1.50", 150, -2, "1.50"),
        ("1.500", 1500, -3, "1.500"),
        ("1E+2", 1, 2, "1E+2"),
        ("100", 100, 0, "100"),
        ("1.00E+2", 100, 0, "100"),
        ("10E+1", 10, 1, "1.0E+2"),
        ("0", 0, 0, "0"),
        ("0.00", 0, -2, "0.00"),
        ("0E+3", 0, 3, "0E+3"),
        ("-0", 0, 0, "-0"),
        ("-1.50", 150, -2, "-1.50"),
        ("-1E+2", 1, 2, "-1E+2"),
        ("0.000001", 1, -6, "0.000001"),
        ("1E-7", 1, -7, "1E-7"),
    ];
    for format in [DfFormat::D16, DfFormat::D34] {
        for (text, coeff, exp, back) in cases {
            let v = DecFloat::parse(text, format).unwrap();
            assert_eq!((v.coefficient(), v.exponent(), v.to_string()), (coeff, exp, back.to_string()), "{text} {format:?}");
            let again = DecFloat::decode(&v.encode(), format).unwrap();
            assert_eq!((again, again.to_string()), (v, back.to_string()), "{text} {format:?}");
            assert_eq!(DecFloat::parse(&v.to_string(), format).unwrap(), v, "{text} {format:?}");
            let neg = v.negated();
            assert_eq!(neg.exponent(), exp, "{text}");
            assert_eq!(DecFloat::decode(&neg.encode(), format).unwrap(), neg, "{text}");
        }
    }
    // Another format keeps the exponent while the digits fit.
    assert_eq!(d34("1.500").to_format(DfFormat::D16).unwrap().to_string(), "1.500");
    assert_eq!(d16("0E+3").to_format(DfFormat::D34).unwrap().to_string(), "0E+3");
    // Equal by value, apart in the total order.
    assert_eq!(d34("1.5").sql_cmp(&d34("1.50")), Ordering::Equal);
    assert_eq!(d34("1E+2").sql_cmp(&d34("100")), Ordering::Equal);
    assert_ne!(d34("1E+2"), d34("100"));
    // Arithmetic's ideal exponents.
    let s = |r: Result<DecFloat>| r.unwrap().to_string();
    assert_eq!(s(add(&d34("1.50"), &d34("1E+2"))), "101.50");
    assert_eq!(s(sub(&d34("1E+2"), &d34("1E+2"))), "0E+2");
    assert_eq!(s(mul(&d34("1.50"), &d34("1E+2"))), "150");
    assert_eq!(s(mul(&d34("1.50"), &d34("1E+3"))), "1.50E+3");
    assert_eq!(s(mul(&d16("1.5"), &d16("2.0"))), "3.00");
    assert_eq!(s(div(&d34("1.50"), &d34("1E+2"))), "0.0150");
    assert_eq!(s(add(&d34("-0"), &d34("-0.00"))), "-0.00");
    // Special values in the standard's spelling.
    assert_eq!(d34("-snan").to_sci_string(), "-sNaN");
    assert_eq!(d34("1E+2").to_sci_string(), "1E+2");
    // A double has no exponent: a whole one takes exponent 0.
    for (x, back) in [(100.0, "100"), (1.5, "1.5"), (0.0, "0"), (-0.0, "-0"), (1e300, "1E+300"), (1e-7, "1E-7")] {
        assert_eq!(DecFloat::from_f64(x, DfFormat::D34).unwrap().to_string(), back, "{x}");
    }
    assert_eq!(DecFloat::from_f64(1e20, DfFormat::D16).unwrap().to_string(), "1E+20");
    assert_eq!(DecFloat::from_f32(250.0, DfFormat::D16).unwrap().to_string(), "250");
    assert_eq!(DecFloat::from_f32(0.1, DfFormat::D16).unwrap().to_string(), "0.1");
}

#[test]
fn rounding_half_even() {
    assert_eq!(d16("1234567890123456.5").to_string(), "1234567890123456");
    assert_eq!(d16("1234567890123457.5").to_string(), "1234567890123458");
    assert_eq!(d16("9999999999999999.5").to_string(), "1.000000000000000E+16");
    assert_eq!(d16("1.00000000000000049").to_string(), "1.000000000000000");
    assert_eq!(d16("1.00000000000000051").to_string(), "1.000000000000001");
    // Subnormal and below.
    assert_eq!(d16("1E-398").to_string(), "1E-398");
    assert_eq!(d16("4E-399").to_string(), "0E-398");
    // Overflow, and the clamp that pads with zeros.
    assert!(DecFloat::parse("1E+385", DfFormat::D16).is_err());
    assert_eq!(d16("1E+384").to_string(), "1.000000000000000E+384");
    assert_eq!(d34("1E+385").to_string(), "1E+385");
}

#[test]
fn arithmetic() {
    let s = |r: Result<DecFloat>| r.unwrap().to_string();
    assert_eq!(s(add(&d34("1.5"), &d34("2.25"))), "3.75");
    assert_eq!(s(sub(&d34("1.50"), &d34("1.5"))), "0.00");
    assert_eq!(s(mul(&d34("1.5"), &d34("-2"))), "-3.0");
    assert_eq!(s(div(&d34("1"), &d34("4"))), "0.25");
    assert_eq!(s(div(&d34("10"), &d34("2"))), "5");
    assert_eq!(s(div(&d34("1"), &d34("3"))), "0.3333333333333333333333333333333333");
    assert_eq!(s(div(&d16("2"), &d16("3"))), "0.6666666666666667");
    assert_eq!(s(add(&d34("1E+40"), &d34("1E-40"))), "1.000000000000000000000000000000000E+40");
    assert_eq!(s(add(&d16("1E+20"), &d16("-1E-20"))), "1.000000000000000E+20");
    assert_eq!(s(sub(&d16("1"), &d16("1E-30"))), "1.000000000000000");
    // The result takes the wider format.
    assert_eq!(add(&d16("1"), &d34("1")).unwrap().format(), DfFormat::D34);
    // Special values.
    assert_eq!(s(add(&d34("Infinity"), &d34("1"))), "Infinity");
    assert_eq!(s(mul(&d34("-Infinity"), &d34("2"))), "-Infinity");
    assert_eq!(s(add(&d34("NaN"), &d34("1"))), "NaN");
    assert_eq!(s(div(&d34("1"), &d34("Infinity"))), "0E-6176");
    // Invalid operations give NaN, as IBM i returns them.
    assert_eq!(s(sub(&d34("Infinity"), &d34("Infinity"))), "NaN");
    assert_eq!(s(mul(&d34("0"), &d34("Infinity"))), "NaN");
    assert!(matches!(add(&d34("sNaN"), &d34("1")), Err(DecimalError::Invalid { .. })));
    assert!(matches!(div(&d34("1"), &d34("0")), Err(DecimalError::DivideByZero)));
    assert!(matches!(div(&d34("0"), &d34("0")), Err(DecimalError::DivideByZero)));
    assert!(matches!(div(&d16("1E-398"), &d16("10")), Err(DecimalError::Underflow { .. })));
    assert!(matches!(mul(&d16("9E+384"), &d16("10")), Err(DecimalError::Overflow { .. })));
}

#[test]
fn comparisons() {
    let order = ["-NaN", "-sNaN", "-Infinity", "-1E+10", "-1", "0", "1E-10", "1", "1E+10", "Infinity", "sNaN", "NaN"];
    for (i, a) in order.iter().enumerate() {
        for (j, b) in order.iter().enumerate() {
            assert_eq!(d34(a).sql_cmp(&d34(b)), i.cmp(&j), "{a} {b}");
        }
    }
    assert_eq!(d34("1.0").sql_cmp(&d16("1.00")), Ordering::Equal);
    assert_eq!(d34("-0").sql_cmp(&d34("0")), Ordering::Equal);
    assert_eq!(d34("-0").total_cmp(&d34("0")), Ordering::Less);
    assert_eq!(d34("1.0").total_cmp(&d34("1.00")), Ordering::Greater);
    assert_eq!(d34("123.45").sql_cmp(&d34("123.46")), Ordering::Less);
}

#[test]
fn encodings() {
    assert_eq!(d16("1").encode(), 0x2238_0000_0000_0001u64.to_be_bytes());
    assert_eq!(d34("1").encode(), 0x2208_0000_0000_0000_0000_0000_0000_0001u128.to_be_bytes());
    assert_eq!(d16("Infinity").encode(), 0x7800_0000_0000_0000u64.to_be_bytes());
    assert_eq!(d16("-Infinity").encode()[0], 0xF8);
    assert_eq!(d16("NaN").encode()[0], 0x7C);
    assert_eq!(d16("sNaN").encode()[0], 0x7E);
    for text in [
        "0", "-0", "1.50", "-9999999999999999", "1E-398", "9.999999999999999E+384", "1234567890123456", "Infinity",
        "-NaN", "sNaN",
    ] {
        let v = d16(text);
        let back = DecFloat::decode(&v.encode(), DfFormat::D16).unwrap();
        assert_eq!(back, v, "{text}");
    }
    for text in ["9999999999999999999999999999999999", "-1.5E-6176", "8.888888888888888888888888888888888E+6144", "0.1"] {
        let v = d34(text);
        assert_eq!(DecFloat::decode(&v.encode(), DfFormat::D34).unwrap(), v, "{text}");
    }
    assert!(DecFloat::decode(&[0; 4], DfFormat::D16).is_err());
}

#[test]
fn conversions() {
    let d = Decimal::parse("-123.4500").unwrap();
    let f = DecFloat::from_decimal(&d, DfFormat::D34).unwrap();
    assert_eq!(f.to_string(), "-123.4500");
    assert_eq!(f.to_decimal().unwrap(), d);
    assert_eq!(d34("1.5E+3").to_decimal().unwrap().to_string(), "1500");
    assert!(d34("NaN").to_decimal().is_err());
    assert!(d34("1E+100").to_decimal().is_err());
    assert_eq!(DecFloat::from_i64(-42, DfFormat::D16).to_string(), "-42");
    assert!((d34("2.5").to_f64() - 2.5).abs() < f64::EPSILON);
    assert_eq!(DecFloat::from_f64(0.1, DfFormat::D16).unwrap().to_string(), "0.1");
    assert_eq!(d34("1.2300").normalized().to_string(), "1.23");
    assert_eq!(d34("1.2E+40").to_format(DfFormat::D16).unwrap().to_string(), "1.2E+40");
}

#[test]
fn rounding_modes() {
    let q = |a: &str, b: &str, m: DfRounding| quantize(&d34(a), &d34(b), m).unwrap().to_string();
    // The primary's QUANTIZE results under each mode.
    assert_eq!(q("2.17", "0.001", DfRounding::HalfEven), "2.170");
    assert_eq!(q("2.15", "0.1", DfRounding::HalfEven), "2.2");
    assert_eq!(q("2.25", "0.1", DfRounding::HalfEven), "2.2");
    assert_eq!(q("2.35", "0.1", DfRounding::HalfEven), "2.4");
    assert_eq!(q("-0.1", "1", DfRounding::HalfEven), "-0");
    assert_eq!(q("217", "1E+1", DfRounding::HalfEven), "2.2E+2");
    assert_eq!(q("2.19", "0.1", DfRounding::Down), "2.1");
    assert_eq!(q("2.25", "0.1", DfRounding::HalfUp), "2.3");
    assert_eq!(q("-2.25", "0.1", DfRounding::HalfUp), "-2.3");
    assert_eq!(q("2.25", "0.1", DfRounding::HalfDown), "2.2");
    assert_eq!(q("2.26", "0.1", DfRounding::HalfDown), "2.3");
    assert_eq!(q("2.21", "0.1", DfRounding::Up), "2.3");
    assert_eq!(q("-2.21", "0.1", DfRounding::Up), "-2.3");
    assert_eq!(q("Infinity", "1", DfRounding::HalfEven), "NaN");
    assert_eq!(q("123456789012345678", "0.1", DfRounding::HalfEven), "123456789012345678.0");
    assert_eq!(q("1E+40", "1", DfRounding::HalfEven), "NaN");
    let d = |a: &str, m: DfRounding| div_in(&d16(a), &d16("3"), m).unwrap().to_string();
    assert_eq!(d("2", DfRounding::Down), "0.6666666666666666");
    assert_eq!(d("-2", DfRounding::Down), "-0.6666666666666666");
    assert_eq!(d("2", DfRounding::Ceiling), "0.6666666666666667");
    assert_eq!(d("-2", DfRounding::Ceiling), "-0.6666666666666666");
    assert_eq!(d("2", DfRounding::Floor), "0.6666666666666666");
    assert_eq!(d("-2", DfRounding::Floor), "-0.6666666666666667");
    assert_eq!(DfRounding::parse("round_half_up"), Some(DfRounding::HalfUp));
    assert_eq!(DfRounding::parse("ROUND_SIDEWAYS"), None);
}

#[test]
fn compare_and_total_order() {
    assert_eq!(compare_total(&d34("1"), &d34("2")), 1);
    assert_eq!(compare_total(&d34("2.0"), &d34("2.00")), 2);
    assert_eq!(compare_total(&d34("2.00"), &d34("2.0")), 1);
    assert_eq!(compare_total(&d34("3"), &d34("2")), 2);
    assert_eq!(compare_total(&d34("NaN"), &d34("2")), 3);
    assert_eq!(compare_total(&d34("2"), &d34("2")), 0);
    assert_eq!(d34("NaN").total_cmp(&d34("sNaN")), Ordering::Greater);
    assert_eq!(d34("-NaN").total_cmp(&d34("-Infinity")), Ordering::Less);
}

#[test]
fn exact_binary_values() {
    let e = |f: f64, fmt: DfFormat| DecFloat::from_f64_exact(f, fmt, DfRounding::HalfEven).unwrap().to_string();
    // The primary's values for these doubles in DECFLOAT(34).
    assert_eq!(e(0.1, DfFormat::D34), "0.1000000000000000055511151231257827");
    assert_eq!(e(1E-30, DfFormat::D34), "1.000000000000000083336420607585985E-30");
    assert_eq!(e(1.0, DfFormat::D34), "1");
    assert_eq!(e(1.5, DfFormat::D16), "1.5");
    assert_eq!(e(1E10, DfFormat::D34), "1E+10");
    assert_eq!(e(-0.0, DfFormat::D34), "-0");
    assert_eq!(e(f64::INFINITY, DfFormat::D16), "Infinity");
}

#[test]
fn encodings_match_the_primary() {
    // HEX() of DECFLOAT columns on the primary (tools/conformance/sql/cases/types/DECFLT1.sql).
    let h = |v: DecFloat| v.encode().iter().fold(String::new(), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02X}");
        s
    });
    assert_eq!(h(d16("1.50")), "22300000000000D0");
    assert_eq!(h(d16("Infinity")), "7800000000000000");
    assert_eq!(h(d16("0")), "2238000000000000");
    assert_eq!(h(d16("12345678901234567")), "263D34B9C1E28E57");
    assert_eq!(h(d34("1.500")), "22074000000000000000000000000680");
    assert_eq!(h(d34("-NaN")), "FC000000000000000000000000000000");
    assert_eq!(h(d34("1.000000000000000083336420607585985E-30")), "25F840000000000000ADB688307B2E8F");
}
