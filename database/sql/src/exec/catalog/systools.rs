// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SYSTOOLS` table functions: `SPLIT`.

use store::txn::Txn;

use super::Table;
use crate::ast::ObjectName;
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;
use crate::exec::eval::{ColInfo, Schema};
use crate::exec::restype::{Kind, SqlType};
use crate::value::Value;

/// Whether `name` is a `SYSTOOLS` table function.
pub(crate) fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_some_and(|s| s.value == "SYSTOOLS") && name.name.value == "SPLIT"
}

fn col(name: &str, t: &SqlType, alias: &str) -> ColInfo {
    ColInfo {
        name: name.into(),
        table: Some(alias.into()),
        nullable: t.nullable,
        field: Some(t.to_field()),
        hidden: false,
    }
}

/// `SYSTOOLS.SPLIT(list, delimiter [, escape])`: the list's elements in
/// order, as `ORDINAL_POSITION` and `ELEMENT`.
pub(crate) fn call(
    _txn: &mut Txn<'_>,
    _ctx: &SqlContext,
    name: &ObjectName,
    args: &[Value],
    _exprs: &[crate::ast::Expr],
    alias: &str,
) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    if !(2..=3).contains(&args.len()) {
        return Err(SqlError::ObjectNotFound {
            name: format!("{name} with {} arguments", args.len()),
            kind: "function",
        });
    }
    let schema = Schema {
        cols: vec![
            col("ORDINAL_POSITION", &SqlType::bigint(true), alias),
            col(
                "ELEMENT",
                &SqlType::new(Kind::Clob, 1 << 20, 0, true),
                alias,
            ),
        ],
    };
    if args[0].is_null() || args[1].is_null() {
        return Ok(Some((schema, Vec::new())));
    }
    let list = args[0].as_text();
    let delim = args[1].as_text();
    let escape = args
        .get(2)
        .filter(|v| !v.is_null())
        .and_then(|v| v.as_text().chars().next());
    let mut parts = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = list.chars().collect();
    let d: Vec<char> = delim.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if Some(chars[i]) == escape && i + 1 < chars.len() {
            cur.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if !d.is_empty() && chars[i..].starts_with(&d) {
            parts.push(std::mem::take(&mut cur));
            i += d.len();
            continue;
        }
        cur.push(chars[i]);
        i += 1;
    }
    parts.push(cur);
    let rows = parts
        .into_iter()
        .enumerate()
        .map(|(k, p)| {
            vec![
                Value::int(i64::try_from(k + 1).expect("fits")),
                Value::Str(p),
            ]
        })
        .collect();
    Ok(Some((schema, rows)))
}
