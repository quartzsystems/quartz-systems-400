// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `JSON_ARRAYAGG(v [FORMAT JSON] [ORDER BY ...] [ABSENT|NULL ON NULL])`
//! and `JSON_OBJECTAGG([KEY] k VALUE v [NULL|ABSENT ON NULL] [WITH
//! UNIQUE KEYS])` over the rows of the group being formed. A null left
//! out is `01003`, as the primary warns.

use std::cmp::Ordering;

use store::txn::Txn;

use super::value::Json;
use super::{Opts, coded, of_value, returned};
use crate::ast::{Function, TypeSpec};
use crate::error::{Result, SqlError};
use crate::exec::eval::{Env, Evaluator};
use crate::value::Value;

pub(super) fn call(
    ev: &mut Evaluator<'_>,
    txn: &mut Txn<'_>,
    f: &Function,
    opts: &Opts,
    ret: Option<&TypeSpec>,
    env: &Env,
) -> Result<Value> {
    let Some(group) = env.group.clone() else {
        return Err(SqlError::Unsupported { what: format!("{} where no group is being formed", f.name) });
    };
    let operands = &f.args[2..];
    let fmt = if opts.or("fmt", "") == "J" { "json" } else { "" };
    // Each row's operands, then its sort keys.
    let mut rows: Vec<(Vec<Value>, Vec<Value>)> = Vec::with_capacity(group.len());
    let width = if f.name == "JSON_OBJECTAGG" { 2 } else { 1 };
    for row in group.iter() {
        ev.ctx.tick(txn)?;
        let e = env.replaced(row.clone());
        let mut vals = Vec::with_capacity(width);
        for x in &operands[..width] {
            vals.push(ev.eval(txn, x, &e)?);
        }
        let mut keys = Vec::new();
        for x in &operands[width..] {
            keys.push(ev.eval(txn, x, &e)?);
        }
        rows.push((vals, keys));
    }
    if rows.is_empty() {
        return Ok(Value::Null);
    }
    let desc: Vec<bool> = opts.or("ord", "").bytes().map(|b| b == b'D').collect();
    let mut err = None;
    rows.sort_by(|a, b| {
        for (i, (x, y)) in a.1.iter().zip(&b.1).enumerate() {
            // Nulls sort high.
            let o = match (x.is_null(), y.is_null()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => x.compare(y).unwrap_or_else(|e| {
                    err.get_or_insert(e);
                    None
                }).unwrap_or(Ordering::Equal),
            };
            let o = if desc.get(i).copied().unwrap_or(false) { o.reverse() } else { o };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    if let Some(e) = err {
        return Err(e);
    }
    let j = if f.name == "JSON_OBJECTAGG" {
        let absent = opts.or("null", "null") == "absent";
        let mut members = Vec::new();
        for (v, _) in &rows {
            if v[0].is_null() {
                return Err(coded(-16401, "22031", "a JSON_OBJECTAGG key is null"));
            }
            if v[1].is_null() {
                ev.ctx.nulls_eliminated = true;
                if absent {
                    continue;
                }
            }
            members.push((v[0].as_text(), of_value(&v[1], fmt)?));
        }
        if opts.get("unique") == Some("1") {
            let mut seen = std::collections::HashSet::new();
            if let Some((k, _)) = members.iter().find(|(k, _)| !seen.insert(k.clone())) {
                return Err(coded(-16400, "22030", format!("the key \"{k}\" is repeated")));
            }
        }
        Json::Object(members)
    } else {
        let absent = opts.or("null", "absent") == "absent";
        let mut items = Vec::new();
        for (v, _) in &rows {
            if v[0].is_null() {
                ev.ctx.nulls_eliminated = true;
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
