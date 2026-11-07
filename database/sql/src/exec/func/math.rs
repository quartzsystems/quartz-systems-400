// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Numeric scalar functions: the logarithms, the trigonometric and
//! hyperbolic functions (as doubles), `SIGN`, `PI`, `RAND`, the `BIT*`
//! functions and `MULTIPLY_ALT`.

use std::cell::Cell;

use qdecimal::{Decimal, Rounding};

use super::super::SqlContext;
use super::super::restype::{Kind, SqlType};
use super::{arity, int, overflow};
use crate::error::{Result, SqlError};
use crate::value::Value;

thread_local! {
    /// `RAND`'s state: a linear congruential generator, seeded by `RAND(n)`.
    static SEED: Cell<u64> = const { Cell::new(0x2545_F491_4F6C_DD1D) };
}

/// The next value of `RAND`, from 0 to 1.
fn next_rand(seed: Option<i64>) -> f64 {
    SEED.with(|s| {
        if let Some(n) = seed {
            s.set(u64::from_ne_bytes(n.to_ne_bytes()) ^ 0x0005_DEEC_E66D);
        }
        let x = s
            .get()
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        s.set(x);
        #[allow(clippy::cast_precision_loss)]
        let f = (x >> 11) as f64 / (1u64 << 53) as f64;
        f
    })
}

fn double(x: f64) -> Result<Value> {
    if x.is_finite() {
        Ok(Value::Double(x))
    } else {
        Err(SqlError::Overflow {
            what: "the result is not a finite number".into(),
        })
    }
}

fn domain(name: &str) -> SqlError {
    SqlError::Overflow {
        what: format!("the argument of {name} is outside its domain"),
    }
}

/// Calls `name` when this family has it.
pub(crate) fn call(name: &str, args: &[Value], _ctx: &SqlContext) -> Option<Result<Value>> {
    let one = |f: fn(f64) -> f64, ok: fn(f64) -> bool| -> Result<Value> {
        arity(name, args, 1, 1)?;
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let x = args[0].as_f64()?;
        if !ok(x) {
            return Err(domain(name));
        }
        double(f(x))
    };
    let any = |_: f64| true;
    Some(match name {
        "EXP" => one(f64::exp, any),
        "LN" | "LOG" => one(f64::ln, |x| x > 0.0),
        "LOG10" => one(|x| x.ln() / std::f64::consts::LN_10, |x| x > 0.0),
        "SIN" => one(f64::sin, any),
        "COS" => one(f64::cos, any),
        "TAN" => one(f64::tan, any),
        "COT" => one(|x| x.cos() / x.sin(), |x| x != 0.0),
        "ASIN" => one(f64::asin, |x| (-1.0..=1.0).contains(&x)),
        "ACOS" => one(f64::acos, |x| (-1.0..=1.0).contains(&x)),
        "ATAN" => one(f64::atan, any),
        "SINH" => one(f64::sinh, any),
        "COSH" => one(f64::cosh, any),
        "TANH" => one(f64::tanh, any),
        "ATANH" => one(f64::atanh, |x| x > -1.0 && x < 1.0),
        "RADIANS" => one(f64::to_radians, any),
        "DEGREES" => one(f64::to_degrees, any),
        "ATAN2" => (|| {
            arity(name, args, 2, 2)?;
            if args.iter().any(Value::is_null) {
                return Ok(Value::Null);
            }
            let (x, y) = (args[0].as_f64()?, args[1].as_f64()?);
            if x == 0.0 && y == 0.0 {
                return Err(domain(name));
            }
            double(y.atan2(x))
        })(),
        "PI" => arity(name, args, 0, 0).map(|()| Value::Double(std::f64::consts::PI)),
        "RAND" | "RANDOM" => (|| {
            arity(name, args, 0, 1)?;
            let seed = match args.first() {
                Some(v) if !v.is_null() => Some(v.as_i64()?),
                _ => None,
            };
            Ok(Value::Double(next_rand(seed)))
        })(),
        "SIGN" => (|| {
            arity(name, args, 1, 1)?;
            Ok(match &args[0] {
                Value::Null => Value::Null,
                Value::Num(d) => {
                    let s = if d.is_zero() {
                        0
                    } else if d.is_negative() {
                        -1
                    } else {
                        1
                    };
                    Value::Num(
                        Decimal::new(s, 0)
                            .expect("small")
                            .rescale(d.scale(), Rounding::Truncate)
                            .map_err(overflow)?,
                    )
                }
                v => {
                    let x = v.as_f64()?;
                    Value::Double(if x == 0.0 { 0.0 } else { x.signum() })
                }
            })
        })(),
        "BITAND" | "BITOR" | "BITXOR" | "BITANDNOT" | "BITNOT" => (|| {
            let n = if name == "BITNOT" { 1 } else { 2 };
            arity(name, args, n, n)?;
            if args.iter().any(Value::is_null) {
                return Ok(Value::Null);
            }
            let a = args[0].as_i64()?;
            let b = if n == 2 { args[1].as_i64()? } else { 0 };
            Ok(int(match name {
                "BITAND" => a & b,
                "BITOR" => a | b,
                "BITXOR" => a ^ b,
                "BITANDNOT" => a & !b,
                _ => !a,
            }))
        })(),
        "MULTIPLY_ALT" => (|| {
            arity(name, args, 2, 2)?;
            if args.iter().any(Value::is_null) {
                return Ok(Value::Null);
            }
            Ok(Value::Num(
                args[0]
                    .as_decimal()?
                    .mul_exact(&args[1].as_decimal()?)
                    .map_err(overflow)?,
            ))
        })(),
        _ => return None,
    })
}

/// The result type of `name` when this family has it.
pub(crate) fn result_type(
    name: &str,
    given: &[Option<SqlType>],
    _exprs: &[crate::ast::Expr],
) -> Option<SqlType> {
    let args = &super::super::restype::rules::known(given)[..];
    let null = args.iter().any(|a| a.nullable);
    let first = args.first().cloned().unwrap_or_else(|| SqlType::int(null));
    Some(match name {
        "EXP" | "LN" | "LOG" | "LOG10" | "SIN" | "COS" | "TAN" | "COT" | "ASIN" | "ACOS"
        | "ATAN" | "ATAN2" | "SINH" | "COSH" | "TANH" | "ATANH" | "RADIANS" | "DEGREES"
        | "POWER" | "POW" | "SQRT" => SqlType::double(null),
        "PI" | "RAND" | "RANDOM" => SqlType::double(false),
        "SIGN" => first.nullable(null),
        "BITAND" | "BITOR" | "BITXOR" | "BITANDNOT" | "BITNOT" => {
            // The wider of the integer types; decimals as BIGINT.
            let rank = |t: &SqlType| match t.kind() {
                Kind::SmallInt => 0,
                Kind::Integer => 1,
                _ => 2,
            };
            match args.iter().map(rank).max().unwrap_or(1) {
                0 => SqlType::smallint(null),
                1 => SqlType::int(null),
                _ => SqlType::bigint(null),
            }
        }
        "MULTIPLY_ALT" => {
            let (p1, s1) = first.digits().unwrap_or((31, 0));
            let (p2, s2) = args.get(1).and_then(SqlType::digits).unwrap_or((31, 0));
            let p = (p1 + p2).min(63);
            let s = u32::from(s1) + u32::from(s2);
            // Past 63 digits the scale gives way, keeping at least 3.
            let s = if p1 + p2 > 63 {
                s.saturating_sub(p1 + p2 - 63).max(3.min(s))
            } else {
                s
            };
            SqlType::decimal(p, u8::try_from(s.min(p)).unwrap_or(0), null)
        }
        "MOD" if args.len() == 2 && !(first.is_integer() && args[1].is_integer()) => {
            let (p1, s1) = first.digits().unwrap_or((31, 0));
            let (p2, s2) = args[1].digits().unwrap_or((31, 0));
            let s = s1.max(s2);
            let p = (p1 - u32::from(s1)).min(p2 - u32::from(s2)) + u32::from(s);
            SqlType::decimal(p.max(1), s, null)
        }
        "ROUND" if first.kind() == Kind::Decimal || first.kind() == Kind::Numeric => {
            SqlType::new(first.kind(), (first.len() + 1).min(63), first.scale(), null)
        }
        "CEILING" | "CEIL" | "FLOOR"
            if first.kind() == Kind::Decimal || first.kind() == Kind::Numeric =>
        {
            SqlType::new(
                first.kind(),
                (first.len() - u32::from(first.scale()) + 1).clamp(1, 63),
                0,
                null,
            )
        }
        _ => return None,
    })
}
