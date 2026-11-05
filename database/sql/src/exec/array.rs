// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arrays (`CREATE TYPE ... AS ARRAY`), in SQL procedures only, as on
//! the primary: the constructor `ARRAY[...]` (kept as the function
//! `*ARRAY`, `*ARRAYQ` for a fullselect), an element `a[i]` (`*ELEMENT`),
//! `CARDINALITY`, `TRIM_ARRAY`, `ARRAY_EXISTS`, `ARRAY_AGG` and `UNNEST`.
//! Elsewhere an array is `SQ20441`. A procedure holds an array as a
//! handle the database maps to its value (`crate::embed::arrays`).

use std::rc::Rc;

use super::eval::{ColInfo, Schema};
use super::SqlContext;
use crate::ast::TypeSpec;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The largest cardinality of an array type declared `ARRAY[]`.
pub const UNBOUNDED: u64 = 2_147_483_647;

/// `SQ20441`: an array where none may be.
#[must_use]
pub fn misplaced() -> SqlError {
    SqlError::Coded { code: -20_441, state: "428H2", what: "an array is not valid here: arrays are for SQL procedures".into() }
}

/// `SQ20439`: an index outside the array.
fn out_of_range(i: &str) -> SqlError {
    SqlError::Coded { code: -20_439, state: "2202E", what: format!("array index {i} is out of range") }
}

/// The index an element is assigned at, from 1 to `max`: `SQ20439`
/// otherwise (SQLSTATE `2200E` for a null one).
///
/// # Errors
///
/// The index is null, not a number, or out of range.
pub fn index(v: &Value, max: u64) -> Result<usize> {
    if v.is_null() {
        return Err(SqlError::Coded { code: -20_439, state: "2200E", what: "array index is null".into() });
    }
    let i = v.as_i64()?;
    match u64::try_from(i) {
        Ok(n) if n >= 1 && n <= max => Ok(usize::try_from(n - 1).unwrap_or(usize::MAX)),
        _ => Err(out_of_range(&i.to_string())),
    }
}

fn items(v: &Value) -> Result<Option<&[Value]>> {
    match v {
        Value::Null => Ok(None),
        Value::Array(a) => Ok(Some(a)),
        _ => Err(SqlError::Coded { code: -171, state: "42815", what: format!("an array is needed, not a {} value", v.type_name()) }),
    }
}

/// The array functions; `None` when `name` is not one.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    if !matches!(name, "*ARRAY" | "*ELEMENT" | "CARDINALITY" | "TRIM_ARRAY" | "ARRAY_EXISTS" | "MAX_CARDINALITY") {
        return None;
    }
    if !ctx.arrays {
        return Some(Err(misplaced()));
    }
    Some(match name {
        "*ARRAY" => Ok(Value::Array(args.to_vec())),
        "*ELEMENT" => element(&args[0], &args[1]),
        "CARDINALITY" => one(name, args).and_then(|a| Ok(items(a)?.map_or(Value::Null, |a| Value::int(i64::try_from(a.len()).unwrap_or(i64::MAX))))),
        "TRIM_ARRAY" => trim(args),
        "ARRAY_EXISTS" => exists(args),
        // A procedure's compiler gives the declared cardinality itself.
        _ => Err(SqlError::Unsupported { what: "MAX_CARDINALITY of an array that is not a variable".into() }),
    })
}

fn one<'a>(name: &str, args: &'a [Value]) -> Result<&'a Value> {
    match args {
        [a] => Ok(a),
        _ => Err(SqlError::Coded { code: -170, state: "42605", what: format!("{name} takes one argument") }),
    }
}

/// `a[i]`: null for a null array or index, `SQ20439` past the end.
fn element(a: &Value, i: &Value) -> Result<Value> {
    let Some(a) = items(a)? else { return Ok(Value::Null) };
    if i.is_null() {
        return Ok(Value::Null);
    }
    let n = i.as_i64()?;
    usize::try_from(n)
        .ok()
        .and_then(|k| k.checked_sub(1))
        .and_then(|k| a.get(k))
        .cloned()
        .ok_or_else(|| out_of_range(&n.to_string()))
}

/// `TRIM_ARRAY(a, n)`: the last `n` elements gone.
fn trim(args: &[Value]) -> Result<Value> {
    let [a, n] = args else {
        return Err(SqlError::Coded { code: -170, state: "42605", what: "TRIM_ARRAY takes two arguments".into() });
    };
    let Some(a) = items(a)? else { return Ok(Value::Null) };
    if n.is_null() {
        return Ok(Value::Null);
    }
    let k = n.as_i64()?;
    let keep = usize::try_from(k).ok().and_then(|k| a.len().checked_sub(k)).ok_or_else(|| out_of_range(&k.to_string()))?;
    Ok(Value::Array(a[..keep].to_vec()))
}

/// `ARRAY_EXISTS(a, i)`: whether `a` has an element `i`.
fn exists(args: &[Value]) -> Result<Value> {
    let [a, i] = args else {
        return Err(SqlError::Coded { code: -170, state: "42605", what: "ARRAY_EXISTS takes two arguments".into() });
    };
    let len = items(a)?.map_or(0, <[Value]>::len);
    if i.is_null() {
        return Ok(Value::Bool(false));
    }
    let n = i.as_i64()?;
    Ok(Value::Bool(usize::try_from(n).is_ok_and(|n| n >= 1 && n <= len)))
}

/// An array value for a variable of an array type: each element cast to
/// the element type, `SQ20439` past the declared cardinality.
///
/// # Errors
///
/// The value is not an array, an element does not convert, or there are
/// too many.
pub fn to_type(v: &Value, elem: &TypeSpec, max: u64) -> Result<Value> {
    let Some(a) = items(v)? else { return Ok(Value::Null) };
    if u64::try_from(a.len()).unwrap_or(u64::MAX) > max {
        return Err(out_of_range(&a.len().to_string()));
    }
    let mut out = Vec::with_capacity(a.len());
    for e in a {
        // Out of the element type's range: SQL0304, as for a variable.
        out.push(super::eval::cast(e, elem).map_err(|e| match e {
            SqlError::Overflow { what } | SqlError::Coded { code: -802, state: "22003", what } => SqlError::Coded { code: -304, state: "22003", what },
            e => e,
        })?);
    }
    Ok(Value::Array(out))
}

/// `UNNEST(a, ...) [WITH ORDINALITY]`: a row per element of the
/// longest array, the shorter ones padded with nulls, the ordinal last.
///
/// # Errors
///
/// An argument is not an array.
pub fn unnest(ctx: &SqlContext, values: &[Value], ordinality: bool) -> Result<(Schema, Vec<Vec<Value>>)> {
    if !ctx.arrays {
        return Err(misplaced());
    }
    let mut lists = Vec::with_capacity(values.len());
    for v in values {
        lists.push(items(v)?.unwrap_or(&[]));
    }
    let n = lists.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut cols: Vec<ColInfo> = (1..=values.len())
        .map(|i| ColInfo { name: format!("{i}"), table: None, field: None, nullable: true, hidden: false })
        .collect();
    if ordinality {
        let field = Some(super::restype::field_of(&super::restype::SqlType::bigint(false)));
        cols.push(ColInfo { name: "ORDINALITY".into(), table: None, field, nullable: false, hidden: false });
    }
    let rows = (0..n)
        .map(|k| {
            let mut r: Vec<Value> = lists.iter().map(|l| l.get(k).cloned().unwrap_or(Value::Null)).collect();
            if ordinality {
                r.push(Value::int(i64::try_from(k + 1).unwrap_or(i64::MAX)));
            }
            r
        })
        .collect();
    Ok((Schema { cols }, rows))
}

/// `ARRAY_AGG`: the group's values in order, nulls kept.
///
/// # Errors
///
/// Arrays are not allowed here.
pub fn aggregate(ctx: &SqlContext, values: Vec<Value>) -> Result<Value> {
    if !ctx.arrays {
        return Err(misplaced());
    }
    Ok(Value::Array(values))
}

/// The rows of `ARRAY[fullselect]`: its first column.
#[must_use]
pub fn from_rows(rows: &Rc<Vec<Vec<Value>>>) -> Value {
    Value::Array(rows.iter().map(|r| r.first().cloned().unwrap_or(Value::Null)).collect())
}
