// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Aggregate functions over the rows of a group or a window frame. The
//! evaluator gathers each row's argument values (and its `WITHIN GROUP`
//! keys); a function here folds them. The basic set is here, the
//! statistics in `stats`, the ordered set (`MEDIAN`, `PERCENTILE_*`,
//! `LISTAGG`) in `ordered`. Later sets (`JSON_ARRAYAGG`, ...) are
//! families listed in [`FAMILIES`] and asked first.

mod ordered;
mod stats;

use std::cmp::Ordering;

use qdecimal::Decimal;

use super::eval::binary;
use super::restype::{Kind, SqlType};
use crate::ast::{BinOp, Expr, Function, OrderItem, Overflow};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// One row's inputs to an aggregate.
#[derive(Clone, Debug)]
pub struct Input {
    /// The argument values.
    pub args: Vec<Value>,
    /// The `WITHIN GROUP` ordering values.
    pub keys: Vec<Value>,
}

/// An aggregate call as the fold sees it.
pub struct Call<'a> {
    /// The name, upper case.
    pub name: &'a str,
    /// `DISTINCT`.
    pub distinct: bool,
    /// `COUNT(*)`.
    pub star: bool,
    /// `WITHIN GROUP (ORDER BY ...)`.
    pub order: &'a [OrderItem],
    /// `LISTAGG ... ON OVERFLOW`.
    pub overflow: Option<&'a Overflow>,
    /// The argument expressions, for a family that reads its clauses
    /// from them.
    pub exprs: &'a [Expr],
}

/// A family of aggregates in its own module.
pub(crate) struct AggFamily {
    /// Whether the family has `name` with this many arguments.
    pub is: fn(&str, usize) -> bool,
    /// The result type, by the argument types and expressions.
    pub ty: super::restype::FunctionType,
    /// Fold the rows; the rows are in `WITHIN GROUP` order, nulls are
    /// the family's to handle (`nulls` set when one is left out).
    pub fold: fn(&Call<'_>, &[Input], &mut bool) -> Option<Result<Value>>,
}

/// The families, asked before the built-in aggregates.
pub(crate) const FAMILIES: &[AggFamily] =
    &[AggFamily { is: super::json::agg::is, ty: super::json::types, fold: super::json::agg::fold }];

/// Built-in aggregates of one argument (or `*`).
const ONE: &[&str] = &[
    "COUNT", "COUNT_BIG", "SUM", "AVG", "MIN", "MAX", "STDDEV", "STDDEV_POP", "STDDEV_SAMP", "VARIANCE", "VAR",
    "VAR_POP", "VAR_SAMP", "VARIANCE_SAMP", "MEDIAN", "ARRAY_AGG", "XMLAGG", "PERCENTILE_CONT", "PERCENTILE_DISC",
];

/// Built-in aggregates of two arguments.
const TWO: &[&str] = &[
    "COVARIANCE", "COVARIANCE_SAMP", "COVAR_POP", "COVAR_SAMP", "COVAR", "CORRELATION", "CORR", "REGR_AVGX",
    "REGR_AVGY", "REGR_COUNT", "REGR_INTERCEPT", "REGR_ICPT", "REGR_R2", "REGR_SLOPE", "REGR_SXX", "REGR_SXY",
    "REGR_SYY",
];

/// Whether `name` with `n` arguments is an aggregate.
#[must_use]
pub fn is_name(name: &str, n: usize, star: bool) -> bool {
    if FAMILIES.iter().any(|f| (f.is)(name, n)) {
        return true;
    }
    if star {
        return matches!(name, "COUNT" | "COUNT_BIG");
    }
    (n == 1 && ONE.contains(&name)) || (n == 2 && TWO.contains(&name)) || (name == "LISTAGG" && (1..=2).contains(&n))
}

/// Whether a call is an aggregate.
#[must_use]
pub fn is_aggregate(f: &Function) -> bool {
    is_name(&f.name, f.args.len(), f.star)
}

/// The aggregate an expression calls directly, with its ordering.
#[must_use]
pub fn call_of(e: &Expr) -> Option<(&Function, &[OrderItem], Option<&Overflow>)> {
    match e {
        Expr::Function(f) if is_aggregate(f) => Some((f, &[], None)),
        Expr::WithinGroup(w) => Some((&w.func, &w.order, w.overflow.as_ref())),
        _ => None,
    }
}

/// The result type of an aggregate call; `None` when it is not one.
#[must_use]
pub fn result_type(f: &Function, given: &[Option<SqlType>]) -> Option<SqlType> {
    if !is_aggregate(f) {
        return None;
    }
    for fam in FAMILIES {
        if (fam.is)(&f.name, given.len()) {
            return (fam.ty)(&f.name, given, &f.args);
        }
    }
    let args = super::restype::rules::known(given);
    let a = args.first().cloned().unwrap_or_else(|| SqlType::int(true));
    Some(match f.name.as_str() {
        "COUNT" => SqlType::int(false),
        "COUNT_BIG" => SqlType::bigint(false),
        "SUM" => sum_type(&a),
        "AVG" => match a.kind() {
            Kind::Decimal | Kind::Numeric => {
                let mp = if a.len() > 31 { 63 } else { 31 };
                let s = (mp + u32::from(a.scale())).saturating_sub(a.len());
                SqlType::decimal(mp, u8::try_from(s).unwrap_or(0), true)
            }
            _ => sum_type(&a),
        },
        "MIN" | "MAX" | "PERCENTILE_DISC" => a.nullable(true),
        "MEDIAN" | "PERCENTILE_CONT" => SqlType::decfloat(true),
        "LISTAGG" => SqlType::varchar(4000, true),
        "REGR_COUNT" => SqlType::bigint(true),
        _ => SqlType::double(true),
    })
}

/// `SUM`'s type: integers stay integers, decimals widen to 31 (or 63).
fn sum_type(a: &SqlType) -> SqlType {
    match a.kind() {
        Kind::SmallInt | Kind::Integer => SqlType::int(true),
        Kind::BigInt => SqlType::bigint(true),
        Kind::Decimal | Kind::Numeric => SqlType::decimal(if a.len() > 31 { 63 } else { 31 }, a.scale(), true),
        Kind::DecFloat => SqlType::decfloat(true),
        _ => SqlType::double(true),
    }
}

fn overflow(e: impl std::fmt::Display) -> SqlError {
    SqlError::Overflow { what: e.to_string() }
}

/// Fold the rows of a group or frame. `nulls` is set when a null value
/// was left out (SQLSTATE `01003`).
pub fn finish(call: &Call<'_>, mut rows: Vec<Input>, nulls: &mut bool) -> Result<Value> {
    if call.name == "XMLAGG" {
        return Err(SqlError::Unsupported { what: "XMLAGG: the XML type".into() });
    }
    if !call.order.is_empty() {
        let specs: Vec<super::sort::KeySpec> = call
            .order
            .iter()
            .map(|o| super::sort::KeySpec { desc: o.desc, nulls_first: o.nulls_first.unwrap_or(o.desc) })
            .collect();
        let mut failure = None;
        rows.sort_by(|a, b| match super::sort::cmp_keys(&a.keys, &b.keys, &specs) {
            Ok(o) => o,
            Err(e) => {
                failure = Some(e);
                Ordering::Equal
            }
        });
        if let Some(e) = failure {
            return Err(e);
        }
    }
    // ARRAY_AGG keeps the nulls, in order (arrays are checked for where
    // the call is evaluated).
    if call.name == "ARRAY_AGG" {
        return Ok(Value::Array(rows.into_iter().map(|r| r.args.into_iter().next().unwrap_or(Value::Null)).collect()));
    }
    for fam in FAMILIES {
        if (fam.is)(call.name, rows.first().map_or(1, |r| r.args.len()))
            && let Some(r) = (fam.fold)(call, &rows, nulls)
        {
            return r;
        }
    }
    if call.star {
        return Ok(Value::int(i64::try_from(rows.len()).expect("fits")));
    }
    // Rows with a null argument are left out (both, for the pairs).
    if call.name.starts_with("PERCENTILE") {
        return ordered::fold(call, &rows);
    }
    let before = rows.len();
    rows.retain(|r| r.args.iter().take(if TWO.contains(&call.name) { 2 } else { 1 }).all(|v| !v.is_null()));
    if rows.len() != before {
        *nulls = true;
    }
    if call.distinct {
        let mut seen = std::collections::HashSet::new();
        rows.retain(|r| seen.insert(r.args[0].key()));
    }
    if TWO.contains(&call.name) {
        return stats::pairs(call.name, &rows);
    }
    let values: Vec<Value> = rows.iter().map(|r| r.args[0].clone()).collect();
    match call.name {
        "COUNT" | "COUNT_BIG" => Ok(Value::int(i64::try_from(values.len()).expect("fits"))),
        "SUM" | "AVG" => sum_avg(call.name == "AVG", &values),
        "MIN" | "MAX" => {
            let want = if call.name == "MIN" { Ordering::Less } else { Ordering::Greater };
            let mut best: Option<Value> = None;
            for v in values {
                best = Some(match best {
                    None => v,
                    Some(b) => {
                        if v.compare(&b)? == Some(want) {
                            v
                        } else {
                            b
                        }
                    }
                });
            }
            Ok(best.unwrap_or(Value::Null))
        }
        "STDDEV" | "STDDEV_POP" | "STDDEV_SAMP" | "VARIANCE" | "VAR" | "VAR_POP" | "VAR_SAMP" | "VARIANCE_SAMP" => {
            stats::spread(call.name, &values)
        }
        "MEDIAN" | "PERCENTILE_CONT" | "PERCENTILE_DISC" | "LISTAGG" => ordered::fold(call, &rows),
        other => Err(SqlError::Unsupported { what: format!("aggregate {other}") }),
    }
}

/// `SUM` and `AVG`: decimals exactly, floats and `DECFLOAT`s in their
/// own arithmetic. A decimal average is carried to 31 places; the result
/// column's type truncates it.
fn sum_avg(avg: bool, values: &[Value]) -> Result<Value> {
    if values.is_empty() {
        return Ok(Value::Null);
    }
    if values.iter().any(|v| matches!(v, Value::DecFloat(_)) || v.is_float()) {
        let mut sum = values[0].clone();
        for v in &values[1..] {
            sum = binary(&sum, BinOp::Add, v)?;
        }
        if avg {
            let n = Value::int(i64::try_from(values.len()).expect("fits"));
            return binary(&sum, BinOp::Div, &n);
        }
        return Ok(sum);
    }
    let mut sum = Decimal::zero();
    for v in values {
        sum = sum.add_exact(&v.as_decimal()?).map_err(overflow)?;
    }
    if avg {
        let n = Decimal::new(i128::try_from(values.len()).expect("fits"), 0).expect("fits");
        let room = 63u8.saturating_sub(sum.digits_needed().saturating_sub(sum.scale()));
        let scale = 31u8.max(sum.scale()).min(room);
        return Ok(Value::Num(sum.div_to_scale(&n, scale).map_err(overflow)?));
    }
    Ok(Value::Num(sum))
}
