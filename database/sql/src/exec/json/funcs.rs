// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scalar JSON functions with clauses.

use store::txn::Txn;

use super::path::{self, PathError};
use super::value::Json;
use super::{Opts, coded, document, of_value, returned};
use crate::ast::{Expr, Function, TypeSpec};
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
    let operands = &f.args[2..];
    match f.name.as_str() {
        "JSON_OBJECT" => object(ev, txn, operands, opts, ret, env),
        "JSON_ARRAY" => array(ev, txn, operands, opts, ret, env),
        name => {
            let item = ev.eval(txn, &operands[0], env)?;
            if item.is_null() {
                return Ok(Value::Null);
            }
            let format = opts.or("in", "auto");
            if name == "IS JSON" {
                return Ok(is_json(&item, opts, format));
            }
            let p = ev.eval(txn, &operands[1], env)?.as_text();
            match name {
                "JSON_VALUE" => json_value(ev, txn, &item, &p, operands, opts, ret, env),
                "JSON_QUERY" => json_query(&item, &p, opts, format),
                _ => json_exists(&item, &p, opts, format),
            }
        }
    }
}

/// The per-value `FORMAT`s: `J` JSON, `B` BSON, `-` none.
fn format_at(opts: &Opts, i: usize) -> &'static str {
    match opts.or("fmt", "").as_bytes().get(i) {
        Some(b'J') => "json",
        Some(b'B') => "bson",
        _ => "",
    }
}

fn object(ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, operands: &[Expr], opts: &Opts, ret: Option<&TypeSpec>, env: &Env) -> Result<Value> {
    let absent = opts.or("null", "null") == "absent";
    let mut members = Vec::with_capacity(operands.len() / 2);
    for (i, pair) in operands.chunks(2).enumerate() {
        let k = ev.eval(txn, &pair[0], env)?;
        if k.is_null() {
            return Err(coded(-16401, "22031", "a JSON_OBJECT key is null"));
        }
        let v = ev.eval(txn, &pair[1], env)?;
        if v.is_null() && absent {
            continue;
        }
        members.push((k.as_text(), of_value(&v, format_at(opts, i))?));
    }
    let j = Json::Object(members);
    if opts.get("unique") == Some("1")
        && let Json::Object(m) = &j
    {
        let mut seen = std::collections::HashSet::new();
        if let Some((k, _)) = m.iter().find(|(k, _)| !seen.insert(k.clone())) {
            return Err(coded(-16400, "22030", format!("the key \"{k}\" is repeated")));
        }
    }
    returned(&j, ret)
}

fn array(ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, operands: &[Expr], opts: &Opts, ret: Option<&TypeSpec>, env: &Env) -> Result<Value> {
    let absent = opts.or("null", "absent") == "absent";
    let mut values = Vec::new();
    if opts.get("sub") == Some("1") {
        let Some(Expr::Subquery(q)) = operands.first() else {
            return Err(coded(-104, "42601", "JSON_ARRAY needs a fullselect"));
        };
        for row in ev.subquery(txn, q, env)?.iter() {
            values.push((row.first().cloned().unwrap_or(Value::Null), ""));
        }
    } else {
        for (i, e) in operands.iter().enumerate() {
            values.push((ev.eval(txn, e, env)?, format_at(opts, i)));
        }
    }
    let mut items = Vec::with_capacity(values.len());
    for (v, fmt) in values {
        if v.is_null() && absent {
            continue;
        }
        items.push(of_value(&v, fmt)?);
    }
    returned(&Json::Array(items), ret)
}

/// A path error as SQL reports it; an unimplemented feature is refused
/// whatever `ON ERROR` says.
fn path_error(e: PathError) -> SqlError {
    match e {
        PathError::Syntax(w) => coded(-16401, "22033", w),
        PathError::Unsupported(w) => SqlError::Unsupported { what: format!("SQL/JSON path {w}") },
        PathError::Structure(w) => coded(-16410, "2203A", w),
    }
}

/// The items of `p` in `item`'s document.
fn items(item: &Value, p: &str, format: &str) -> Result<(Json, path::Path)> {
    let path = path::parse(p).map_err(path_error)?;
    Ok((document(item, format)?, path))
}

/// Whether an error is one `ON ERROR` handles (not a refusal).
fn handled(e: &SqlError) -> bool {
    !matches!(e, SqlError::Unsupported { .. })
}

/// A scalar's SQL text: a string as it is, a number in its decimal form
/// (`1.5e2` is `150`), `true`/`false`; `None` for `null`.
fn scalar_text(j: &Json) -> Option<String> {
    match j {
        Json::Str(s) => Some(s.clone()),
        Json::Num(n) => Some(
            qdecimal::DecFloat::parse(n, qdecimal::DfFormat::D34)
                .ok()
                .and_then(|d| d.to_decimal().ok())
                .map_or_else(|| n.clone(), |d| d.to_string()),
        ),
        Json::Bool(b) => Some(if *b { "true" } else { "false" }.into()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn json_value(
    ev: &mut Evaluator<'_>,
    txn: &mut Txn<'_>,
    item: &Value,
    p: &str,
    operands: &[Expr],
    opts: &Opts,
    ret: Option<&TypeSpec>,
    env: &Env,
) -> Result<Value> {
    let format = opts.or("in", "auto");
    let found = (|| -> Result<Option<Value>> {
        let (doc, path) = items(item, p, format)?;
        let mut found: Vec<&Json> = path.eval(&doc).map_err(path_error)?;
        // Lax: an array of one scalar is that scalar.
        if !path.strict
            && found.len() == 1
            && let Json::Array(inner) = found[0]
            && inner.len() == 1
            && inner[0].is_scalar()
        {
            found = vec![&inner[0]];
        }
        match found.as_slice() {
            [] => Ok(None),
            [one] if one.is_scalar() => {
                let text = scalar_text(one);
                Ok(Some(match text {
                    None => Value::Null,
                    Some(t) => match ret {
                        None => Value::Str(t),
                        Some(spec) => crate::exec::cast::cast(&Value::Str(t), spec, ev.ctx.decfloat_rounding)
                            .map_err(|e| coded(-406, "22023", e.to_string()))?,
                    },
                }))
            }
            [_] => Err(coded(-16413, "2203F", "the SQL/JSON item is not a scalar")),
            _ => Err(coded(-16413, "22034", "the path gives more than one SQL/JSON item")),
        }
    })();
    let default = |ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, e: &Expr| -> Result<Value> {
        let v = ev.eval(txn, e, env)?;
        match ret {
            Some(spec) => crate::exec::cast::cast(&v, spec, ev.ctx.decfloat_rounding),
            None => Ok(Value::Str(v.as_text())).map(|s| if v.is_null() { Value::Null } else { s }),
        }
    };
    match found {
        Ok(Some(v)) => Ok(v),
        Ok(None) => match opts.or("empty", "null") {
            "error" => Err(coded(-16410, "22035", "the path gives no SQL/JSON item")),
            "default" => default(ev, txn, &operands[2]),
            _ => Ok(Value::Null),
        },
        Err(e) if handled(&e) => match opts.or("error", "null") {
            "error" => Err(e),
            "default" => default(ev, txn, &operands[3]),
            _ => Ok(Value::Null),
        },
        Err(e) => Err(e),
    }
}

fn json_query(item: &Value, p: &str, opts: &Opts, format: &str) -> Result<Value> {
    let r = (|| -> Result<Option<String>> {
        let (doc, path) = items(item, p, format)?;
        let found = path.eval(&doc).map_err(path_error)?;
        if found.is_empty() {
            return Ok(None);
        }
        let wrap = match opts.or("wrap", "none") {
            "uncond" => true,
            "cond" => found.len() != 1 || found[0].is_scalar(),
            _ => false,
        };
        if wrap {
            return Ok(Some(Json::Array(found.into_iter().cloned().collect()).text()));
        }
        if found.len() > 1 {
            return Err(coded(-16413, "22034", "the path gives more than one SQL/JSON item; add WITH WRAPPER"));
        }
        Ok(Some(match (found[0], opts.or("quotes", "keep")) {
            (Json::Str(s), "omit") => s.clone(),
            (j, _) => j.text(),
        }))
    })();
    let fallback = |how: &str| match how {
        "array" => Ok(Value::Str("[]".into())),
        "object" => Ok(Value::Str("{}".into())),
        _ => Ok(Value::Null),
    };
    match r {
        Ok(Some(t)) => Ok(Value::Str(t)),
        Ok(None) => match opts.or("empty", "null") {
            "error" => Err(coded(-16410, "22035", "the path gives no SQL/JSON item")),
            how => fallback(how),
        },
        Err(e) if handled(&e) => match opts.or("error", "null") {
            "error" => Err(e),
            how => fallback(how),
        },
        Err(e) => Err(e),
    }
}

fn json_exists(item: &Value, p: &str, opts: &Opts, format: &str) -> Result<Value> {
    let r = items(item, p, format).and_then(|(doc, path)| path.eval(&doc).map(|f| !f.is_empty()).map_err(path_error));
    match r {
        Ok(b) => Ok(Value::Bool(b)),
        Err(e) if handled(&e) => {
            match opts.or("error", "false") {
                "error" => Err(e),
                "true" => Ok(Value::Bool(true)),
                "unknown" => Ok(Value::Null),
                _ => Ok(Value::Bool(false)),
            }
        }
        Err(e) => Err(e),
    }
}

fn is_json(item: &Value, opts: &Opts, format: &str) -> Value {
    let Ok(doc) = document(item, format) else {
        return Value::Bool(false);
    };
    let kind = match opts.or("kind", "value") {
        "array" => matches!(doc, Json::Array(_)),
        "object" => matches!(doc, Json::Object(_)),
        "scalar" => doc.is_scalar(),
        _ => true,
    };
    let unique = opts.get("unique") != Some("1") || !doc.has_duplicate_keys();
    Value::Bool(kind && unique)
}
