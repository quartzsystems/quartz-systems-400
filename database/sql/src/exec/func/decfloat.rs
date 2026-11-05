// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `DECFLOAT` functions (`DECFLOAT`, `NORMALIZE_DECFLOAT`,
//! `QUANTIZE`, `COMPARE_DECFLOAT`, `TOTALORDER`, `DECFLOAT_FORMAT`) and
//! `CHAR`/`VARCHAR` of a number in IBM's text. Results round in
//! `CURRENT DECFLOAT ROUNDING MODE`.

use std::cmp::Ordering;

use qdecimal::{DecFloat, DfFormat};

use super::super::SqlContext;
use super::super::cast::{char_text, decfloat_error, to_decfloat};
use super::arity;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The family.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    let f = match name {
        "DECFLOAT" => decfloat,
        "NORMALIZE_DECFLOAT" => normalize,
        "QUANTIZE" => quantize,
        "COMPARE_DECFLOAT" => compare,
        "TOTALORDER" => total_order,
        "DECFLOAT_FORMAT" => format,
        "HEX" if matches!(args.first(), Some(Value::Bool(_))) => {
            return Some(Ok(Value::Str(if matches!(args[0], Value::Bool(true)) { "F1" } else { "F0" }.into())));
        }
        "CHAR" | "VARCHAR" if args.first().is_some_and(is_number) => {
            return Some(char_of(name, args));
        }
        _ => return None,
    };
    if args.iter().any(Value::is_null) {
        return Some(arity(name, args, 1, 3).map(|()| Value::Null));
    }
    Some(f(name, args, ctx))
}

fn is_number(v: &Value) -> bool {
    matches!(v, Value::Num(_) | Value::Double(_) | Value::Real(_) | Value::DecFloat(_))
}

/// The `DECFLOAT` of either operand's width: 34 unless both are 16.
fn df(v: &Value, ctx: &SqlContext) -> Result<DecFloat> {
    let format = match v {
        Value::DecFloat(d) => d.format(),
        _ => DfFormat::D34,
    };
    to_decfloat(v, format, ctx.decfloat_rounding)
}

/// `DECFLOAT(x [, 16 | 34 [, decimal character]])`.
fn decfloat(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 1, 3)?;
    let format = match args.get(1).map(Value::as_i64).transpose()? {
        None | Some(34) => DfFormat::D34,
        Some(16) => DfFormat::D16,
        Some(n) => {
            return Err(SqlError::Coded {
                code: -171,
                state: "42815",
                what: format!("argument 2 of DECFLOAT is {n}, not 16 or 34"),
            });
        }
    };
    let v = match (&args[0], args.get(2)) {
        (Value::Str(s), Some(dc)) => Value::Str(s.replace(dc.as_text().as_str(), ".")),
        (v, _) => v.clone(),
    };
    to_decfloat(&v, format, ctx.decfloat_rounding).map(Value::DecFloat)
}

fn normalize(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 1, 1)?;
    Ok(Value::DecFloat(df(&args[0], ctx)?.normalized()))
}

fn quantize(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 2, 2)?;
    let (a, b) = (df(&args[0], ctx)?, df(&args[1], ctx)?);
    qdecimal::decfloat::quantize(&a, &b, ctx.decfloat_rounding).map(Value::DecFloat).map_err(decfloat_error)
}

fn compare(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 2, 2)?;
    let (a, b) = (df(&args[0], ctx)?, df(&args[1], ctx)?);
    Ok(Value::int(i64::from(qdecimal::decfloat::compare_total(&a, &b))))
}

fn total_order(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 2, 2)?;
    let (a, b) = (df(&args[0], ctx)?, df(&args[1], ctx)?);
    Ok(Value::int(match a.total_cmp(&b) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }))
}

/// `DECFLOAT_FORMAT(text [, format])`: the format's `9`, `0`, `.`, `,`,
/// `$`, `S`, `MI` and `PR` elements read off the text.
fn format(name: &str, args: &[Value], ctx: &SqlContext) -> Result<Value> {
    arity(name, args, 1, 2)?;
    let text = args[0].as_text();
    let t = text.trim();
    let mut neg = false;
    let mut body = t.to_string();
    if let Some(fmt) = args.get(1).map(Value::as_text) {
        let f = fmt.trim().to_ascii_uppercase();
        if f.chars().any(|c| !"90.,$SMIPR".contains(c)) {
            return Err(SqlError::Coded {
                code: -20448,
                state: "22007",
                what: format!("'{fmt}' is not a DECFLOAT_FORMAT format"),
            });
        }
        if f.ends_with("PR") && body.starts_with('<') && body.ends_with('>') {
            neg = true;
            body = body[1..body.len() - 1].to_string();
        }
        if f.ends_with("MI") && body.ends_with('-') {
            neg = true;
            body.pop();
        }
        body = body.replace([',', '$'], "");
    }
    if let Some(r) = body.strip_prefix('-') {
        neg = !neg;
        body = r.to_string();
    }
    let v = to_decfloat(&Value::Str(body), DfFormat::D34, ctx.decfloat_rounding)?;
    Ok(Value::DecFloat(if neg { v.negated() } else { v }))
}

/// `CHAR(number [, decimal character])` and `VARCHAR`: IBM's text of
/// the number; a `CHAR` of a float is 24 characters, of a `DECFLOAT` 42.
fn char_of(name: &str, args: &[Value]) -> Result<Value> {
    arity(name, args, 1, 2)?;
    if args.iter().any(Value::is_null) {
        return Ok(Value::Null);
    }
    let mut s = char_text(&args[0]);
    if let Some(dc) = args.get(1) {
        let dc = dc.as_text();
        if dc.chars().count() != 1 || dc.chars().any(|c| c.is_ascii_digit() || c == ' ' || c == '+' || c == '-') {
            return Err(SqlError::Coded {
                code: -171,
                state: "42815",
                what: format!("argument 2 of {name} is '{dc}', not a decimal character"),
            });
        }
        s = s.replacen('.', &dc, 1);
    }
    let width = match &args[0] {
        Value::Double(_) | Value::Real(_) => 24,
        Value::DecFloat(_) => 42,
        _ => 0,
    };
    if name == "CHAR" && s.chars().count() < width {
        let pad = width - s.chars().count();
        s.extend(std::iter::repeat_n(' ', pad));
    }
    Ok(Value::Str(s))
}
