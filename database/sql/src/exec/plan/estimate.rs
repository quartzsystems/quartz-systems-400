// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How many rows a predicate keeps, from the column's statistics where
//! they have been collected, else from fixed shares: a tenth for an
//! equality, a third for a range, a quarter for a range bounded on both
//! sides. A value the statement gives only when it runs (a parameter,
//! a host variable) takes the fixed share too.

use super::stats::ColumnStats;
use crate::ast::{CmpOp, Expr};
use crate::value::Value;

/// The share an equality keeps without statistics.
pub const EQUAL: f64 = 0.1;
/// The share a one-sided range keeps.
pub const RANGE: f64 = 1.0 / 3.0;
/// The share a range bounded on both sides keeps.
pub const BETWEEN: f64 = 0.25;
/// The share any other predicate keeps.
pub const OTHER: f64 = 0.5;

/// The value of a literal operand, when it is one.
#[must_use]
pub fn constant(e: &Expr) -> Option<Value> {
    match e {
        Expr::Literal(l) => super::super::eval::literal(l).ok().filter(|v| !v.is_null()),
        Expr::Neg(a) => match constant(a)? {
            Value::Num(d) => Some(Value::Num(d.negated())),
            _ => None,
        },
        _ => None,
    }
}

/// The share of rows `col op value` keeps.
#[must_use]
pub fn compare(stats: Option<&ColumnStats>, op: CmpOp, value: Option<&Value>) -> f64 {
    let (Some(s), Some(v)) = (stats, value) else {
        return match op {
            CmpOp::Eq => EQUAL,
            CmpOp::Ne => 1.0 - EQUAL,
            _ => RANGE,
        };
    };
    match op {
        CmpOp::Eq => s.equal(v),
        CmpOp::Ne => (1.0 - s.equal(v)).max(0.0),
        CmpOp::Lt => s.below(v, false),
        CmpOp::Le => s.below(v, true),
        CmpOp::Gt => (nonnull(s) - s.below(v, true)).max(0.0),
        CmpOp::Ge => (nonnull(s) - s.below(v, false)).max(0.0),
    }
}

/// The share of rows a range keeps: values from `lo`, up to `hi`
/// (each inclusive or not), `None` for an open side or a value not
/// known until the statement runs (`Some(None)`).
#[must_use]
pub fn range(stats: Option<&ColumnStats>, lo: Option<(Option<&Value>, bool)>, hi: Option<(Option<&Value>, bool)>) -> f64 {
    let known = |b: Option<(Option<&Value>, bool)>| b.is_none_or(|(v, _)| v.is_some());
    let Some(s) = stats.filter(|_| known(lo) && known(hi)) else {
        return match (lo, hi) {
            (Some(_), Some(_)) => BETWEEN,
            (None, None) => 1.0,
            _ => RANGE,
        };
    };
    let top = hi.and_then(|(v, incl)| v.map(|v| s.below(v, incl))).unwrap_or_else(|| nonnull(s));
    let bottom = lo.and_then(|(v, incl)| v.map(|v| s.below(v, !incl))).unwrap_or(0.0);
    (top - bottom).clamp(0.0, 1.0)
}

#[allow(clippy::cast_precision_loss)]
fn nonnull(s: &ColumnStats) -> f64 {
    if s.rows == 0 { 0.0 } else { 1.0 - s.nulls as f64 / s.rows as f64 }
}

/// Rows out of `rows` at share `f`, at least one when any.
#[must_use]
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn rows(rows: u64, f: f64) -> u64 {
    if rows == 0 {
        return 0;
    }
    ((rows as f64 * f).round() as u64).clamp(1, rows)
}
