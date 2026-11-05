// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! JSON in SQL: the constructors (`JSON_OBJECT`, `JSON_ARRAY`), the
//! queries (`JSON_VALUE`, `JSON_QUERY`, `JSON_EXISTS`, `IS JSON`), the
//! aggregates (`JSON_ARRAYAGG`, `JSON_OBJECTAGG`), BSON
//! (`JSON_TO_BSON`, `BSON_TO_JSON`) and the evaluator of `JSON_TABLE`.
//! The parser (`parse/json.rs`) writes each call as a function whose
//! first argument holds its clauses ([`Opts`]) and whose second is the
//! `RETURNING` type (a `CAST` of `NULL`) or `NULL`; the rest are its
//! operands. The primary for all of it is pub400
//! (`tools/conformance/sql/cases/types/JSON*.sql`).

pub mod agg;
pub mod bson;
mod funcs;
pub mod path;
pub mod table;
pub mod value;

use store::txn::Txn;

use super::eval::{Env, Evaluator};
use super::restype::SqlType;
use crate::ast::{DataType, Expr, Function, Literal, TypeSpec};
use crate::error::{Result, SqlError};
use crate::value::Value;

pub use value::Json;

/// The functions written with JSON clauses.
pub const CLAUSED: &[&str] =
    &["JSON_OBJECT", "JSON_ARRAY", "JSON_VALUE", "JSON_QUERY", "JSON_EXISTS", "IS JSON", "JSON_ARRAYAGG", "JSON_OBJECTAGG"];

/// The clauses of a call: `name=value` pairs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opts(Vec<(String, String)>);

impl Opts {
    /// A clause's value.
    #[must_use]
    pub fn get(&self, k: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
    }

    /// A clause's value, or `default`.
    #[must_use]
    pub fn or<'a>(&'a self, k: &str, default: &'a str) -> &'a str {
        self.get(k).unwrap_or(default)
    }

    /// Set a clause.
    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        self.0.retain(|(n, _)| n != k);
        self.0.push((k.into(), v.into()));
    }

    /// The text kept in the call.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.0 {
            s.push_str(k);
            s.push('=');
            s.push_str(v);
            s.push(';');
        }
        s
    }

    /// From that text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self(
            text.split(';')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    /// The options of a call the parser wrote.
    fn of(f: &Function) -> Self {
        match f.args.first() {
            Some(Expr::Literal(Literal::Str(s))) => Self::parse(s),
            _ => Self::default(),
        }
    }
}

/// The `RETURNING` type of a call the parser wrote.
fn returning(f: &Function) -> Option<&TypeSpec> {
    match f.args.get(1) {
        Some(Expr::Cast(_, t)) => Some(t),
        _ => None,
    }
}

/// An error with IBM's code.
pub(crate) fn coded(code: i32, state: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Coded { code, state, what: what.into() }
}

/// `SQ16402`: text that is not JSON.
pub(crate) fn not_json(what: &str) -> SqlError {
    coded(-16402, "22032", format!("{what} is not valid JSON"))
}

/// Whether `f` is one of the JSON aggregates.
#[must_use]
pub fn is_aggregate(f: &Function) -> bool {
    matches!(f.name.as_str(), "JSON_ARRAYAGG" | "JSON_OBJECTAGG") && matches!(f.args.first(), Some(Expr::Literal(_)))
}

/// Evaluate `f` when it is a JSON call the parser wrote; `None` for any
/// other function.
pub fn call(ev: &mut Evaluator<'_>, txn: &mut Txn<'_>, f: &Function, env: &Env) -> Option<Result<Value>> {
    if !CLAUSED.contains(&f.name.as_str()) || !matches!(f.args.first(), Some(Expr::Literal(Literal::Str(_)))) {
        return None;
    }
    let opts = Opts::of(f);
    let ret = returning(f).cloned();
    Some(match f.name.as_str() {
        "JSON_ARRAYAGG" | "JSON_OBJECTAGG" => agg::call(ev, txn, f, &opts, ret.as_ref(), env),
        _ => funcs::call(ev, txn, f, &opts, ret.as_ref(), env),
    })
}

/// The scalar functions without clauses: `JSON_TO_BSON`, `BSON_TO_JSON`.
pub(crate) fn plain(name: &str, args: &[Value], _ctx: &super::SqlContext) -> Option<Result<Value>> {
    let f: fn(&Value) -> Result<Value> = match name {
        "JSON_TO_BSON" => |v| match v {
            Value::Str(s) => value::parse(s).map(|j| Value::Bin(bson::encode(&j))).map_err(|_| not_json("the argument")),
            v => Err(coded(-171, "42815", format!("argument 1 of JSON_TO_BSON is {}, not character", v.type_name()))),
        },
        "BSON_TO_JSON" => |v| match v {
            Value::Bin(b) => bson::decode(b)
                .map(|j| {
                    let mut s = String::new();
                    j.write(&mut s, true);
                    Value::Str(s)
                })
                .map_err(|_| coded(-16402, "22032", "the argument is not valid BSON")),
            v => Err(coded(-171, "42815", format!("argument 1 of BSON_TO_JSON is {}, not binary", v.type_name()))),
        },
        _ => return None,
    };
    if args.len() != 1 {
        return Some(Err(coded(-170, "42605", format!("{name} takes one argument"))));
    }
    if args[0].is_null() {
        return Some(Ok(Value::Null));
    }
    Some(f(&args[0]))
}

/// The result types of the JSON functions.
#[must_use]
pub fn types(name: &str, args: &[Option<SqlType>], exprs: &[Expr]) -> Option<SqlType> {
    let clob = || SqlType { spec: plain_spec(DataType::Clob(i32::MAX.unsigned_abs())), nullable: true };
    let ret = || match exprs.get(1) {
        Some(Expr::Cast(_, t)) => Some(SqlType { spec: t.clone(), nullable: true }),
        _ => None,
    };
    let claused = matches!(exprs.first(), Some(Expr::Literal(Literal::Str(_))));
    Some(match name {
        "JSON_OBJECT" | "JSON_ARRAY" | "JSON_VALUE" | "JSON_QUERY" | "JSON_ARRAYAGG" | "JSON_OBJECTAGG" if claused => {
            ret().unwrap_or_else(clob)
        }
        "JSON_EXISTS" | "IS JSON" if claused => {
            let nullable = args.get(2).and_then(Option::as_ref).is_none_or(|t| t.nullable);
            SqlType { spec: plain_spec(DataType::Boolean), nullable: nullable && name == "IS JSON" }
        }
        "BSON_TO_JSON" => SqlType { spec: plain_spec(DataType::Clob(i32::MAX.unsigned_abs())), nullable: args.first()?.as_ref()?.nullable },
        "JSON_TO_BSON" => SqlType { spec: plain_spec(DataType::Blob(i32::MAX.unsigned_abs())), nullable: args.first()?.as_ref()?.nullable },
        _ => return None,
    })
}

fn plain_spec(t: DataType) -> TypeSpec {
    TypeSpec { data_type: t, ccsid: None, bit_data: false }
}

/// A document from a context item: JSON text, or BSON in a binary
/// string; `FORMAT JSON` reads binary as UTF-8 text.
pub(crate) fn document(v: &Value, format: &str) -> Result<Json> {
    match (v, format) {
        (Value::Bin(b), "json") => {
            let t = String::from_utf8(b.clone()).map_err(|_| not_json("the binary value"))?;
            value::parse(&t).map_err(|_| not_json("the value"))
        }
        (Value::Bin(b), _) => bson::decode(b).map_err(|_| not_json("the BSON value")),
        (Value::Str(s), "bson") => Err(not_json(&format!("'{s}' (FORMAT BSON needs a binary string)"))),
        (Value::Str(s), _) => value::parse(s).map_err(|_| not_json("the value")),
        (v, _) => Err(coded(-171, "42815", format!("a {} value is not a JSON document", v.type_name()))),
    }
}

/// An SQL value as JSON, as the constructors write it: numbers in IBM's
/// text (`1.50`, `1.5E0`, `1.0E+3`), dates and times as ISO strings,
/// `FORMAT JSON` text as it is, `FORMAT BSON` decoded.
pub(crate) fn of_value(v: &Value, format: &str) -> Result<Json> {
    Ok(match (v, format) {
        (Value::Null, _) => Json::Null,
        (Value::Str(s), "json") => Json::Raw(s.clone()),
        (Value::Bin(b), "bson") => bson::decode(b).map_err(|_| not_json("the BSON value"))?,
        (Value::Num(d), _) => Json::Num(d.to_string()),
        // A negative zero keeps its sign here, unlike CHAR.
        (Value::Double(x), _) if *x == 0.0 && x.is_sign_negative() => Json::Num("-0E0".into()),
        (Value::Double(x), _) if x.is_finite() => Json::Num(super::cast::float_char(*x)),
        (Value::Real(x), _) if x.is_finite() => Json::Num(super::cast::float_char(f64::from(*x))),
        (Value::DecFloat(d), _) if d.is_finite() => Json::Num(d.to_sci_string()),
        (Value::Bool(b), _) => Json::Bool(*b),
        (Value::Str(s), _) => Json::Str(s.clone()),
        (Value::Date(_) | Value::Time(_), _) => Json::Str(v.to_string()),
        (Value::Timestamp(t), _) => Json::Str(t.render_field()),
        (v, _) => return Err(not_json(&format!("a {} value", v.type_name()))),
    })
}

/// The text a constructor gives in its `RETURNING` type: too long for a
/// string type is `SQL0802` (`22001`), as the primary reports it; a
/// binary type takes the BSON.
pub(crate) fn returned(j: &Json, ret: Option<&TypeSpec>) -> Result<Value> {
    let text = j.text();
    let Some(t) = ret else { return Ok(Value::Str(text)) };
    let max = match t.data_type {
        DataType::Char(n) | DataType::Varchar(n) | DataType::Clob(n) | DataType::Graphic(n) | DataType::Vargraphic(n) | DataType::Dbclob(n) => n,
        DataType::Varbinary(n) | DataType::Blob(n) | DataType::Binary(n) => {
            let parsed = value::parse(&text).map_err(|_| not_json("the result"))?;
            let b = bson::encode(&parsed);
            if b.len() > usize::try_from(n).unwrap_or(usize::MAX) {
                return Err(coded(-802, "22001", "the BSON result is longer than its type"));
            }
            return Ok(Value::Bin(b));
        }
        _ => return Err(coded(-171, "42815", "RETURNING needs a character or binary string type")),
    };
    if text.chars().count() > usize::try_from(max).unwrap_or(usize::MAX) {
        return Err(coded(-802, "22001", format!("the JSON result is longer than {max} characters")));
    }
    Ok(Value::Str(text))
}
