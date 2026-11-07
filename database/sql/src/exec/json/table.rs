// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `JSON_TABLE`'s rows: one per item of the row path, a column per
//! `COLUMNS` entry (`FOR ORDINALITY`, a value by its path with `ON
//! EMPTY` and `ON ERROR`, or `FORMAT JSON` text).
//! `JSON_TABLE` in `FROM` is a table function of `exec::catalog`: the
//! parser writes its columns as arguments (`parse::json`), `call` reads
//! them back.

use qdecimal::DfRounding;
use store::txn::Txn;

use super::path;
use super::value::Json;
use super::{coded, document};
use crate::ast::{Expr, ObjectName, TypeSpec};
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;
use crate::exec::catalog::Table;
use crate::exec::eval::{ColInfo, Schema};
use crate::exec::restype::SqlType;
use crate::value::Value;

/// What a column gives when its path finds nothing or fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// `NULL ON ...` (the default).
    Null,
    /// `ERROR ON ...`.
    Error,
    /// `DEFAULT value ON ...`, the value evaluated.
    Default(Value),
}

/// A `COLUMNS` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Column {
    /// `name FOR ORDINALITY`: the row's number from 1 (`BIGINT`).
    Ordinality,
    /// `name type [FORMAT JSON] [PATH 'p'] [... ON EMPTY] [... ON ERROR]`.
    Value {
        /// The column's type.
        spec: TypeSpec,
        /// The path (`$.name` when not given, the name in lower case
        /// is IBM's default).
        path: String,
        /// `FORMAT JSON`: the item's JSON text.
        json: bool,
        /// `ON EMPTY`.
        empty: Fallback,
        /// `ON ERROR`.
        error: Fallback,
    },
}

fn fallback(f: &Fallback, e: SqlError) -> Result<Value> {
    match f {
        Fallback::Null => Ok(Value::Null),
        Fallback::Error => Err(e),
        Fallback::Default(v) => Ok(v.clone()),
    }
}

/// One column's value for the item `item`.
fn column(c: &Column, item: &Json, n: usize, mode: DfRounding) -> Result<Value> {
    let Column::Value {
        spec,
        path: p,
        json,
        empty,
        error,
    } = c
    else {
        return Ok(Value::int(i64::try_from(n).unwrap_or(i64::MAX)));
    };
    let path = path::parse(p).map_err(|e| SqlError::Unsupported {
        what: format!("the JSON_TABLE path {p}: {e:?}"),
    })?;
    let found = match path.eval(item) {
        Ok(f) => f,
        Err(e) => return fallback(error, coded(-16410, "2203A", format!("{e:?}"))),
    };
    let one = match found.as_slice() {
        [] => {
            return fallback(
                empty,
                coded(-16410, "22035", "the column path gives no item"),
            );
        }
        [one] => *one,
        many if *json => {
            return Ok(Value::Str(
                Json::Array(many.iter().map(|j| (*j).clone()).collect()).text(),
            ));
        }
        _ => {
            return fallback(
                error,
                coded(-16413, "22034", "the column path gives more than one item"),
            );
        }
    };
    if *json {
        return Ok(Value::Str(one.text()));
    }
    let text = match one {
        Json::Null => return Ok(Value::Null),
        Json::Str(s) => s.clone(),
        Json::Num(n) => n.clone(),
        Json::Bool(b) => if *b { "true" } else { "false" }.into(),
        _ => {
            return fallback(
                error,
                coded(-16413, "2203F", "the column's item is not a scalar"),
            );
        }
    };
    let target = Value::Str(text);
    match crate::exec::cast::cast(&target, spec, mode) {
        Ok(v) => Ok(v),
        // SQL0406 when the error stands, as pub400.
        Err(e) => fallback(error, coded(-406, "22023", e.to_string())),
    }
}

/// The rows of `JSON_TABLE(item, row_path COLUMNS(columns))`. A context
/// item that is not JSON, or a row path that fails, gives no rows
/// (`EMPTY ON ERROR`, the default) or the error (`ERROR ON ERROR`), as
/// pub400 (`types/JSONTERR`).
pub fn rows(
    item: &Value,
    row_path: &str,
    columns: &[Column],
    mode: DfRounding,
    error_on_error: bool,
) -> Result<Vec<Vec<Value>>> {
    if item.is_null() {
        return Ok(Vec::new());
    }
    let rp = path::parse(row_path).map_err(|e| SqlError::Unsupported {
        what: format!("the JSON_TABLE row path {row_path}: {e:?}"),
    })?;
    let doc = match document(item, "auto") {
        Ok(d) => d,
        Err(e) if error_on_error => return Err(e),
        Err(_) => return Ok(Vec::new()),
    };
    let items = match rp.eval(&doc) {
        Ok(i) => i,
        Err(e) if error_on_error => return Err(coded(-16410, "2203A", format!("{e:?}"))),
        Err(_) => return Ok(Vec::new()),
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, it) in items.into_iter().enumerate() {
        let mut row = Vec::with_capacity(columns.len());
        for c in columns {
            row.push(column(c, it, i + 1, mode)?);
        }
        out.push(row);
    }
    Ok(out)
}

/// Whether `name` is `JSON_TABLE`.
pub(crate) fn has(name: &ObjectName) -> bool {
    name.schema.is_none() && name.name.value == "JSON_TABLE"
}

fn fallback_of(how: Option<&str>, default: &Value) -> Fallback {
    match how {
        Some("error") => Fallback::Error,
        Some("default") => Fallback::Default(default.clone()),
        _ => Fallback::Null,
    }
}

/// The columns and rows of a `JSON_TABLE` the parser wrote.
pub(crate) fn call(
    _txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: &[Value],
    exprs: &[Expr],
    alias: &str,
) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    let (Some(item), Some(row_path)) = (args.first(), args.get(1)) else {
        return Ok(None);
    };
    let mut columns = Vec::new();
    let mut cols = Vec::new();
    // `ERROR ON ERROR` for the table: the parser's last entry. A column's
    // own `ON ERROR` default is then ERROR too.
    let kind = |c: &[Value]| {
        c.first().and_then(|o| {
            super::Opts::parse(&o.as_text())
                .get("k")
                .map(str::to_string)
        })
    };
    let error_on_error = args[2..]
        .chunks(6)
        .any(|c| kind(c).as_deref() == Some("terr"));
    for (i, c) in args[2..].chunks(6).enumerate() {
        let [opts, cname, path, _, empty, error] = c else {
            return Ok(None);
        };
        let opts = super::Opts::parse(&opts.as_text());
        if opts.get("k") == Some("terr") {
            continue;
        }
        let column_error = match opts.get("error") {
            None if error_on_error => Fallback::Error,
            how => fallback_of(how, error),
        };
        let (column, t) = if opts.get("k") == Some("ord") {
            (Column::Ordinality, SqlType::bigint(true))
        } else {
            let Some(Expr::Cast(_, spec)) = exprs.get(2 + i * 6 + 3) else {
                return Ok(None);
            };
            let column = Column::Value {
                spec: spec.clone(),
                path: path.as_text(),
                json: opts.get("fmt") == Some("J"),
                empty: fallback_of(opts.get("empty"), empty),
                error: column_error,
            };
            (
                column,
                SqlType {
                    spec: spec.clone(),
                    nullable: true,
                },
            )
        };
        columns.push(column);
        cols.push(ColInfo {
            name: cname.as_text(),
            table: Some(alias.into()),
            nullable: t.nullable,
            field: Some(t.to_field()),
            hidden: false,
        });
    }
    let rows = rows(
        item,
        &row_path.as_text(),
        &columns,
        ctx.decfloat_rounding,
        error_on_error,
    )?;
    Ok(Some((Schema { cols }, rows)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DataType;

    fn col(t: DataType, p: &str) -> Column {
        Column::Value {
            spec: TypeSpec {
                data_type: t,
                ccsid: None,
                bit_data: false,
                udt: None,
            },
            path: p.into(),
            json: false,
            empty: Fallback::Null,
            error: Fallback::Null,
        }
    }

    #[test]
    fn rows_as_pub400_gives_them() {
        let doc = Value::Str(r#"{"r":[{"a":1,"b":"x"},{"a":2}]}"#.into());
        let cols = [
            col(DataType::Integer, "$.a"),
            col(DataType::Varchar(5), "$.b"),
            Column::Ordinality,
        ];
        let r = rows(&doc, "$.r[*]", &cols, DfRounding::HalfEven, false).unwrap();
        assert_eq!(
            r,
            vec![
                vec![Value::int(1), Value::Str("x".into()), Value::int(1)],
                vec![Value::int(2), Value::Null, Value::int(2)],
            ]
        );
        let doc = Value::Str(r#"{"r":[{"a":1,"b":"x"},{"a":"q"}]}"#.into());
        let cols = [
            Column::Value {
                spec: TypeSpec {
                    data_type: DataType::Integer,
                    ccsid: None,
                    bit_data: false,
                    udt: None,
                },
                path: "$.a".into(),
                json: false,
                empty: Fallback::Null,
                error: Fallback::Default(Value::int(-1)),
            },
            Column::Value {
                spec: TypeSpec {
                    data_type: DataType::Varchar(20),
                    ccsid: None,
                    bit_data: false,
                    udt: None,
                },
                path: "$".into(),
                json: true,
                empty: Fallback::Null,
                error: Fallback::Null,
            },
        ];
        let r = rows(&doc, "lax $.r[*]", &cols, DfRounding::HalfEven, false).unwrap();
        assert_eq!(
            r[1],
            vec![Value::int(-1), Value::Str(r#"{"a":"q"}"#.into())]
        );
    }
}
