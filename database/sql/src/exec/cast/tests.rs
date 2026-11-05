// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The casting matrix against the primary's outcomes
//! (`tools/conformance/sql/cases/types/CASTSTR.sql`, `DECFLT2.sql`).

use super::*;
use crate::value::parse_date;

fn spec(t: DataType) -> TypeSpec {
    TypeSpec { data_type: t, ccsid: None, bit_data: false }
}

#[allow(clippy::needless_pass_by_value)]
fn c(v: Value, t: DataType) -> String {
    cast(&v, &spec(t), DfRounding::HalfEven).map_or_else(|e| e.msgid(), |v| char_text(&v))
}

fn s(t: &str) -> Value {
    Value::Str(t.into())
}

fn n(t: &str) -> Value {
    Value::Num(qdecimal::Decimal::parse(t).unwrap())
}

#[test]
fn strings_to_numbers() {
    assert_eq!(c(s(" 12 "), DataType::Integer), "12");
    assert_eq!(c(s("12.7"), DataType::Integer), "12");
    assert_eq!(c(s("1E2"), DataType::Integer), "100");
    assert_eq!(c(s("1E2"), DataType::Decimal(5, 0)), "100");
    assert_eq!(c(s("1.239"), DataType::Decimal(5, 2)), "1.23");
    assert_eq!(c(s(" -1.5e-1 "), DataType::Double), "-1.4999999999999999E-1");
    assert_eq!(c(s("abc"), DataType::Decimal(5, 0)), "SQL0420");
    assert_eq!(c(s(""), DataType::Integer), "SQL0420");
    assert_eq!(c(s("Infinity"), DataType::Double), "SQL0420");
    assert_eq!(c(s("123456"), DataType::Decimal(5, 0)), "SQL0802");
    assert_eq!(c(s("1E400"), DataType::Double), "SQL0802");
    assert_eq!(c(s(" 1.5 "), DataType::DecFloat(16)), "1.5");
    assert_eq!(c(s("abc"), DataType::DecFloat(34)), "SQL0420");
    assert_eq!(c(s("-sNaN"), DataType::DecFloat(34)), "-SNAN");
}

#[test]
fn numbers_to_numbers() {
    assert_eq!(c(n("1.239"), DataType::Decimal(5, 2)), "1.23");
    assert_eq!(c(n("-1.239"), DataType::Decimal(5, 2)), "-1.23");
    assert_eq!(c(Value::Double(1.5), DataType::Decimal(5, 0)), "1");
    assert_eq!(c(Value::Double(-1.5), DataType::Integer), "-1");
    assert_eq!(c(n("9223372036854775808"), DataType::BigInt), "SQL0802");
    assert_eq!(c(n("9223372036854775807"), DataType::BigInt), "9223372036854775807");
    assert_eq!(c(n("32768"), DataType::SmallInt), "SQL0802");
    let inf = Value::DecFloat(DecFloat::parse("Infinity", DfFormat::D16).unwrap());
    assert_eq!(c(inf, DataType::Integer), "SQL0802");
    assert_eq!(c(Value::Double(0.1), DataType::DecFloat(34)), "0.1000000000000000055511151231257827");
    assert_eq!(c(Value::Double(0.1), DataType::DecFloat(16)), "0.1");
    assert_eq!(c(Value::Double(1E10), DataType::DecFloat(34)), "1E+10");
}

#[test]
fn values_to_strings() {
    assert_eq!(c(n("12"), DataType::Char(5)), "12   ");
    assert_eq!(c(n("-12.50"), DataType::Char(8)), "-12.50  ");
    assert_eq!(c(n("0.5"), DataType::Char(4)), ".5  ");
    assert_eq!(c(n("-0.5"), DataType::Varchar(5)), "-.5");
    assert_eq!(c(n("123456"), DataType::Char(3)), "SQL0433");
    assert_eq!(c(Value::Double(1.5), DataType::Char(3)), "SQL0433");
    assert_eq!(c(Value::Double(1.5), DataType::Char(5)), "1.5E0");
    assert_eq!(c(s("abcdef"), DataType::Char(3)), "abc");
    assert_eq!(c(s("abcdef"), DataType::Varchar(3)), "abc");
    assert_eq!(c(n("1"), DataType::Graphic(3)), "1  ");
    assert_eq!(c(n("1"), DataType::Clob(10)), "1");
    assert_eq!(c(Value::Bool(false), DataType::Char(5)), "FALSE");
}

#[test]
fn binary_and_datetime() {
    assert_eq!(c(s("AB"), DataType::Binary(4)), "C1C20000");
    assert_eq!(c(s("AB"), DataType::Varbinary(4)), "C1C2");
    assert_eq!(c(n("1"), DataType::Binary(4)), "SQL0461");
    assert_eq!(c(Value::Bin(vec![1]), DataType::Integer), "SQL0461");
    let d = Value::Date(parse_date("2026-11-05").unwrap());
    assert_eq!(c(d.clone(), DataType::Integer), "SQL0461");
    assert_eq!(c(d, DataType::Decimal(8, 0)), "SQL0461");
    assert_eq!(c(n("20261105"), DataType::Date), "SQL0461");
    assert_eq!(c(s("2026-13-05"), DataType::Date), "SQL0181");
    assert_eq!(c(s("abc"), DataType::Date), "SQL0180");
    assert_eq!(c(s("25:00:00"), DataType::Time), "SQL0181");
    assert_eq!(c(Value::Bin(vec![1]), DataType::Vargraphic(3)), "SQL0332");
}

#[test]
fn ccsids() {
    let with = |t: DataType, ccsid: u16| {
        cast(&s("abc"), &TypeSpec { data_type: t, ccsid: Some(ccsid), bit_data: false }, DfRounding::HalfEven)
            .map_or_else(|e| e.msgid(), |v| v.as_text())
    };
    assert_eq!(with(DataType::Graphic(3), 13488), "abc");
    assert_eq!(with(DataType::Vargraphic(5), 1200), "abc");
    assert_eq!(with(DataType::Char(3), 37), "abc");
    assert_eq!(with(DataType::Char(3), 500), "abc");
    assert_eq!(with(DataType::Char(3), 1208), "abc");
    assert_eq!(with(DataType::Char(3), 819), "SQL0189");
    assert_eq!(with(DataType::Char(3), 9999), "SQL0189");
    assert_eq!(with(DataType::Char(3), 1200), "SQL0189");
    assert_eq!(with(DataType::Graphic(3), 37), "SQL0189");
}
