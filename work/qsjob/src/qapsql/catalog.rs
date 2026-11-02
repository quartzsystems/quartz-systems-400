// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ListSchemas`, `ListTables` and `ListColumns` from the QSYS2 catalog
//! views, read with the user's authority on a secondary thread's own
//! link: `SYSSCHEMAS`, `SYSTABLES` (indexes left out) and, for a
//! table's columns, a `DESCRIBE` of `SELECT *` from it (the same
//! authority check, types, SQL names and headings as `Prepare`).
//! Patterns are matched and pages cut here, names in order.

use qap_proto::Status;
use qap_proto::ops::sql::{Columns, ListColumns, ListSchemas, ListTables, Schema, Schemas, Table, TableKind, Tables};
use qapd::Refusal;
use qapd::sql::{like, page};
use sql::embed::{Desc, Op, Request};
use sql::value::Value;

use super::{Engine, sql_message, values};

/// The embedded scope of the catalog's statements.
const SCOPE: &str = "QAPCAT:LIST";

fn refused(e: &mut dyn Engine, ca: &sql::Sqlca) -> Refusal {
    Refusal {
        status: if ca.sqlcode == -551 { Status::NotAuthorized } else { Status::Error },
        message: sql_message(e, ca),
    }
}

fn run(e: &mut dyn Engine, op: Op, name: &str, text: &str, inputs: Vec<Value>) -> sql::embed::Reply {
    e.run(&Request {
        scope: SCOPE.into(),
        desc: Desc {
            op,
            name: name.into(),
            sql_naming: true,
            rows: op == Op::Fetch,
            into: op == Op::Prepare,
            text: text.into(),
            ..Desc::default()
        },
        inputs,
    })
}

/// Every row of a catalog query.
fn rows(e: &mut dyn Engine, text: &str) -> Result<Vec<Vec<Value>>, Refusal> {
    let r = run(e, Op::Open, "QAPCAT", text, Vec::new());
    if r.sqlca.is_error() {
        return Err(refused(e, &r.sqlca));
    }
    let f = run(e, Op::Fetch, "QAPCAT", "", vec![Value::int(i64::from(i32::MAX))]);
    let _ = run(e, Op::Close, "QAPCAT", "", Vec::new());
    if f.sqlca.is_error() {
        return Err(refused(e, &f.sqlca));
    }
    Ok(f.rows)
}

/// A string literal.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A column's text without the padding.
fn text(row: &[Value], i: usize) -> String {
    match row.get(i) {
        None | Some(Value::Null) => String::new(),
        Some(v) => v.as_text().trim_end().to_string(),
    }
}

fn not_found(e: &mut dyn Engine, name: &str, schema: &str) -> Refusal {
    Refusal {
        status: Status::NotFound,
        message: e.message("SQL0204", &[name, schema]),
    }
}

/// `ListSchemas`.
pub fn list_schemas(e: &mut dyn Engine, req: &ListSchemas) -> Result<Schemas, Refusal> {
    let mut all: Vec<Schema> = rows(e, "SELECT SCHEMA_NAME, SCHEMA_TEXT FROM QSYS2.SYSSCHEMAS")?
        .iter()
        .map(|r| Schema {
            name: text(r, 0),
            text: text(r, 1),
        })
        .filter(|s| like(&s.name, &req.pattern))
        .collect();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    let (total, schemas) = page(all, req.start, req.max);
    Ok(Schemas { total, schemas })
}

/// `ListTables`: `NotFound` for a schema that is not there.
pub fn list_tables(e: &mut dyn Engine, req: &ListTables) -> Result<Tables, Refusal> {
    let schema = req.schema.trim();
    let q = quote(schema);
    if rows(e, &format!("SELECT SCHEMA_NAME FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME = {q}"))?.is_empty() {
        return Err(not_found(e, schema, "*LIB"));
    }
    let found = rows(e, &format!("SELECT TABLE_NAME, TABLE_TYPE, TABLE_TEXT FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = {q}"))?;
    let mut all: Vec<Table> = found
        .iter()
        .filter_map(|r| {
            let kind = match text(r, 1).as_str() {
                "T" => TableKind::Table,
                "V" => TableKind::View,
                "A" => TableKind::Alias,
                "P" => TableKind::PhysicalFile,
                "L" => TableKind::LogicalFile,
                // Indexes are not tables.
                _ => return None,
            };
            Some(Table {
                schema: schema.to_string(),
                name: text(r, 0),
                kind,
                text: text(r, 2),
            })
        })
        .filter(|t| like(&t.name, &req.pattern) && (req.kinds.is_empty() || req.kinds.contains(&t.kind)))
        .collect();
    all.sort_by(|a, b| a.name.cmp(&b.name));
    let (total, tables) = page(all, req.start, req.max);
    Ok(Tables { total, tables })
}

/// `ListColumns`: `NotFound`, or `NotAuthorized` as a `SELECT` of the
/// table would be.
pub fn list_columns(e: &mut dyn Engine, req: &ListColumns) -> Result<Columns, Refusal> {
    let (schema, table) = (req.schema.trim(), req.table.trim());
    let (qs, qt) = (quote(schema), quote(table));
    let there = rows(e, &format!("SELECT TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = {qs} AND TABLE_NAME = {qt}"))?;
    if there.is_empty() {
        return Err(not_found(e, table, schema));
    }
    let ident = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let select = format!("SELECT * FROM {}.{}", ident(schema), ident(table));
    let p = run(e, Op::Prepare, "QAPCATS", &select, Vec::new());
    if p.sqlca.is_error() {
        return Err(refused(e, &p.sqlca));
    }
    let columns = p.rows.iter().map(|row| values::described(row).column).collect();
    Ok(Columns { columns })
}
