// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The type rules of an operator whose operands differ in kind: a
//! character string beside a number takes the number's type, `DECFLOAT`
//! arithmetic rounds in `CURRENT DECFLOAT ROUNDING MODE`, a number
//! concatenated is its IBM text and a date, time or timestamp may not be
//! (`SQL0171`). Everything else is [`crate::exec::eval::binary`].

use qdecimal::{DfFormat, DfRounding};

use super::{char_text, coded, decfloat_error, number, to_decfloat};
use crate::ast::BinOp;
use crate::error::Result;
use crate::exec::eval;
use crate::value::Value;

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_))
}

/// A string beside `other` (a number) as a value of `other`'s type: a
/// decimal's scale (`1 + '1.5'` is 2, `1.0 + '1.5'` is 2.5), a float, a
/// `DECFLOAT(34)`.
pub fn string_as(s: &str, other: &Value) -> Result<Value> {
    Ok(match other {
        Value::Num(d) => Value::Num(number::parse(s)?.decimal(d.scale(), s)?),
        Value::Double(_) | Value::Real(_) => {
            number::parse(s)?;
            Value::Double(s.trim_matches(' ').parse().map_err(|_| super::bad_chars(s))?)
        }
        _ => Value::DecFloat(to_decfloat(&Value::Str(s.to_string()), DfFormat::D34, DfRounding::HalfEven)?),
    })
}

/// `a op b` with the rounding mode for `DECFLOAT` results.
#[allow(clippy::many_single_char_names)]
pub fn binary(a: &Value, op: BinOp, b: &Value, mode: DfRounding) -> Result<Value> {
    if a.is_null() || b.is_null() {
        return Ok(Value::Null);
    }
    if op == BinOp::Concat {
        let datetime = |v: &Value| matches!(v, Value::Date(_) | Value::Time(_) | Value::Timestamp(_));
        if datetime(a) || datetime(b) {
            let n = if datetime(a) { 1 } else { 2 };
            return Err(coded(-171, "42815", format!("argument {n} of CONCAT is a date, time or timestamp")));
        }
        if is_number(a) || is_number(b) {
            return Ok(Value::Str(format!("{}{}", char_text(a), char_text(b))));
        }
        return eval::binary(a, op, b);
    }
    if matches!(a, Value::Bool(_)) || matches!(b, Value::Bool(_)) {
        return Err(coded(-402, "42819", "a BOOLEAN operand of an arithmetic operator"));
    }
    // A string beside a number takes the number's type.
    let (a, b) = match (a, b) {
        (Value::Str(s), o) if is_number(o) => (string_as(s, o)?, o.clone()),
        (o, Value::Str(s)) if is_number(o) => (o.clone(), string_as(s, o)?),
        _ => (a.clone(), b.clone()),
    };
    if matches!(a, Value::DecFloat(_)) || matches!(b, Value::DecFloat(_)) {
        let format = a.decfloat_format().max(b.decfloat_format());
        let x = to_decfloat(&a, format_beside(&a, format), mode)?;
        let y = to_decfloat(&b, format_beside(&b, format), mode)?;
        let r = match op {
            BinOp::Add => qdecimal::decfloat::add_in(&x, &y, mode),
            BinOp::Sub => qdecimal::decfloat::sub_in(&x, &y, mode),
            BinOp::Mul => qdecimal::decfloat::mul_in(&x, &y, mode),
            BinOp::Div => qdecimal::decfloat::div_in(&x, &y, mode),
            BinOp::Concat => unreachable!("handled above"),
        };
        return r.map(Value::DecFloat).map_err(decfloat_error);
    }
    eval::binary(&a, op, &b)
}

/// The format an operand converts to: a `DECFLOAT` keeps its own, a
/// number takes the result's.
fn format_beside(v: &Value, result: DfFormat) -> DfFormat {
    match v {
        Value::DecFloat(d) => d.format(),
        _ => result,
    }
}
