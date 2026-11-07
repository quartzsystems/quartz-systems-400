// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `JSON_ARRAYAGG(v [FORMAT JSON] [ORDER BY ...] [ABSENT|NULL ON NULL])`
//! and `JSON_OBJECTAGG([KEY] k VALUE v [NULL|ABSENT ON NULL] [WITH
//! UNIQUE KEYS])`: a family of `exec::agg`, folded over the rows of a
//! group or a window frame. A null left out is `01003`, as the primary
//! warns.

use std::cmp::Ordering;

use super::value::Json;
use super::{Opts, coded, of_value, returned};
use crate::ast::{Expr, Literal};
use crate::error::{Result, SqlError};
use crate::exec::agg::{Call, Input};
use crate::value::Value;

/// Whether `name` is one of the JSON aggregates.
pub(crate) fn is(name: &str, _args: usize) -> bool {
    matches!(name, "JSON_ARRAYAGG" | "JSON_OBJECTAGG")
}

/// The aggregate over the rows: each row's arguments are the clauses
/// (`Opts` text), the `RETURNING` cast, the operands, then the sort keys.
pub(crate) fn fold(call: &Call<'_>, rows: &[Input], nulls: &mut bool) -> Option<Result<Value>> {
    if !is(call.name, 0) {
        return None;
    }
    Some(over(call, rows, nulls))
}

fn over(call: &Call<'_>, rows: &[Input], nulls: &mut bool) -> Result<Value> {
    let opts = match call.exprs.first() {
        Some(Expr::Literal(Literal::Str(s))) => Opts::parse(s),
        _ => Opts::default(),
    };
    let ret = match call.exprs.get(1) {
        Some(Expr::Cast(_, t)) => Some(t),
        _ => None,
    };
    let fmt = if opts.or("fmt", "") == "J" {
        "json"
    } else {
        ""
    };
    let width = if call.name == "JSON_OBJECTAGG" { 2 } else { 1 };
    let mut rows: Vec<(&[Value], &[Value])> = rows
        .iter()
        .map(|r| {
            let operands = r.args.get(2..).unwrap_or(&[]);
            operands.split_at(width.min(operands.len()))
        })
        .collect();
    if rows.is_empty() {
        return Ok(Value::Null);
    }
    let desc: Vec<bool> = opts.or("ord", "").bytes().map(|b| b == b'D').collect();
    let mut err: Option<SqlError> = None;
    rows.sort_by(|a, b| {
        for (i, (x, y)) in a.1.iter().zip(b.1).enumerate() {
            // Nulls sort high.
            let o = match (x.is_null(), y.is_null()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => x
                    .compare(y)
                    .unwrap_or_else(|e| {
                        err.get_or_insert(e);
                        None
                    })
                    .unwrap_or(Ordering::Equal),
            };
            let o = if desc.get(i).copied().unwrap_or(false) {
                o.reverse()
            } else {
                o
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    if let Some(e) = err {
        return Err(e);
    }
    let j = if call.name == "JSON_OBJECTAGG" {
        let absent = opts.or("null", "null") == "absent";
        let mut members = Vec::new();
        for (v, _) in &rows {
            if v[0].is_null() {
                return Err(coded(-16401, "22031", "a JSON_OBJECTAGG key is null"));
            }
            if v[1].is_null() {
                *nulls = true;
                if absent {
                    continue;
                }
            }
            members.push((v[0].as_text(), of_value(&v[1], fmt)?));
        }
        if opts.get("unique") == Some("1") {
            let mut seen = std::collections::HashSet::new();
            if let Some((k, _)) = members.iter().find(|(k, _)| !seen.insert(k.clone())) {
                return Err(coded(
                    -16400,
                    "22030",
                    format!("the key \"{k}\" is repeated"),
                ));
            }
        }
        Json::Object(members)
    } else {
        let absent = opts.or("null", "absent") == "absent";
        let mut items = Vec::new();
        for (v, _) in &rows {
            if v[0].is_null() {
                *nulls = true;
                if absent {
                    continue;
                }
            }
            items.push(of_value(&v[0], fmt)?);
        }
        Json::Array(items)
    };
    returned(&j, ret)
}
