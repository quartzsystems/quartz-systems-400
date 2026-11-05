// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data changes as a query or a statement of their own: the rows a
//! change wrote or replaced as a table (`FINAL TABLE`, `NEW TABLE`,
//! `OLD TABLE`), and `TRUNCATE`.

use store::txn::Txn;

use super::dml;
use super::eval::{Row, Schema};
use super::query::{row_of, table_schema};
use super::SqlContext;
use crate::ast::{ChangeRows, Delete, Statement, Truncate, Where};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// Run the change, keeping the records it wrote (or, for `OLD TABLE`,
/// the ones it replaced), and give them as rows of the table.
pub(crate) fn data_change_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    kind: ChangeRows,
    statement: &Statement,
    alias: &str,
    explain_only: bool,
) -> Result<(Schema, Vec<Row>)> {
    let name = match statement {
        Statement::Insert(i) => &i.table,
        Statement::Update(u) => &u.table,
        Statement::Delete(d) => &d.table,
        _ => return Err(SqlError::Unsupported { what: "a data-change table reference of that statement".into() }),
    };
    // DB2 for i: no OLD TABLE of an INSERT, no FINAL or NEW TABLE of a
    // DELETE.
    let allowed = !matches!(
        (kind, statement),
        (ChangeRows::Old, Statement::Insert(_)) | (ChangeRows::Final | ChangeRows::New, Statement::Delete(_))
    );
    if !allowed {
        return Err(SqlError::Misplaced { what: "that data-change table reference for the statement".into() });
    }
    let t = ctx.resolve_table(txn, name)?;
    let schema = table_schema(&t, alias);
    if explain_only {
        ctx.explain.push(format!("DATA CHANGE {kind:?} ({alias})"));
        return Ok((schema, Vec::new()));
    }
    let saved = (ctx.captured.replace(Vec::new()), ctx.capture_old);
    ctx.capture_old = kind == ChangeRows::Old;
    let r = match statement {
        Statement::Insert(i) => dml::insert(txn, ctx, params, i),
        Statement::Update(u) => dml::update(txn, ctx, params, u),
        Statement::Delete(d) => dml::delete(txn, ctx, params, d),
        _ => unreachable!("checked"),
    };
    let captured = std::mem::replace(&mut ctx.captured, saved.0).unwrap_or_default();
    ctx.capture_old = saved.1;
    r?;
    let mut rows = Vec::with_capacity(captured.len());
    for (rrn, record, null_map) in captured {
        rows.push(row_of(txn, &t, &db::Slot { rrn, record, null_map, deleted: false })?);
    }
    Ok((schema, rows))
}

/// `TRUNCATE`: every row deleted as `DELETE` without `WHERE` deletes
/// them, the count not reported.
pub(crate) fn truncate(txn: &mut Txn<'_>, ctx: &mut SqlContext, params: &[Value], t: &Truncate) -> Result<()> {
    let info = ctx.resolve_table(txn, &t.table)?;
    if t.restart_identity && info.format.format.fields.iter().any(|f| f.identity.is_some()) {
        // BLOCKED(sql): restarting an identity column needs SQL-2's identity state.
        return Err(SqlError::Unsupported { what: "TRUNCATE ... RESTART IDENTITY of a table with an identity column".into() });
    }
    let d = Delete { table: t.table.clone(), alias: None, where_clause: Where::All, isolation: None };
    dml::delete(txn, ctx, params, &d)?;
    Ok(())
}

/// A record a change kept: its RRN, the record and its null map.
pub(crate) type Captured = (u64, Vec<u8>, Vec<u8>);
