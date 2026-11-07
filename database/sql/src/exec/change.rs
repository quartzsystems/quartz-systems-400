// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data changes as a query or a statement of their own: the rows a
//! change wrote or replaced as a table (`FINAL TABLE`, `NEW TABLE`,
//! `OLD TABLE`), and `TRUNCATE`.

use store::txn::Txn;

use super::SqlContext;
use super::dml;
use super::eval::Env;
use super::eval::{Row, Schema};
use super::query::{row_of, run_query, table_schema};
use crate::ast::{
    ChangeRows, Delete, Query, QueryBody, Select, SelectItem, Statement, TableRef, Truncate, Where,
};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// Run the `INSERT`, keeping the records it wrote, and give them as
/// rows of the table: `FINAL TABLE` and `NEW TABLE` (no trigger here
/// changes them between the two).
pub(crate) fn data_change_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    kind: ChangeRows,
    statement: &Statement,
    alias: &str,
    explain_only: bool,
) -> Result<(Schema, Vec<Row>)> {
    let Statement::Insert(ins) = statement else {
        return Err(SqlError::Unsupported {
            what: "a data-change table reference of a statement other than INSERT".into(),
        });
    };
    let t = ctx.resolve_table(txn, &ins.table)?;
    let schema = table_schema(&t, alias);
    if explain_only {
        ctx.explain.push(format!("DATA CHANGE {kind:?} ({alias})"));
        return Ok((schema, Vec::new()));
    }
    let saved = ctx.captured.replace(Vec::new());
    let r = dml::insert(txn, ctx, params, ins);
    let captured = std::mem::replace(&mut ctx.captured, saved).unwrap_or_default();
    r?;
    let mut rows = Vec::with_capacity(captured.len());
    for (rrn, record, null_map) in captured {
        rows.push(row_of(
            txn,
            &t,
            &db::Slot {
                rrn,
                record,
                null_map,
                deleted: false,
            },
        )?);
    }
    Ok((schema, rows))
}

/// `TRUNCATE`: every row deleted as `DELETE` without `WHERE` deletes
/// them, the count not reported.
pub(crate) fn truncate(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &Truncate,
) -> Result<u64> {
    let info = ctx.resolve_table(txn, &t.table)?;
    // `RESTART IDENTITY`: the identity column starts again at its START.
    let restart = if t.restart_identity {
        let kept = db::sqlinf::seq::columns(txn, info.id)?
            .into_iter()
            .find_map(|c| match c.kind {
                db::sqlinf::seq::ColumnKind::Identity(a) => Some(a.start),
                _ => None,
            });
        kept.or_else(|| {
            info.format
                .format
                .fields
                .iter()
                .find_map(|f| f.identity.as_ref().map(|i| i128::from(i.start)))
        })
    } else {
        None
    };
    let d = Delete {
        table: t.table.clone(),
        alias: None,
        where_clause: Where::All,
        isolation: None,
    };
    // No row to delete is the warning SQL0100.
    let n = dml::delete(txn, ctx, params, &d)?;
    if let Some(start) = restart {
        super::column::set_counter(txn, info.id, start)?;
    }
    Ok(n)
}

/// A record a change kept: its RRN, the record and its null map.
pub(crate) type Captured = (u64, Vec<u8>, Vec<u8>);

/// For a joined `MERGE` source, the correlation name of each column of
/// `SELECT *` over it, in order; `None` for any other source.
pub(crate) fn source_tables(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    source: &TableRef,
) -> Result<Option<Vec<String>>> {
    fn leaves<'a>(t: &'a TableRef, out: &mut Vec<&'a TableRef>) -> Result<()> {
        match t {
            TableRef::Join { left, right, .. } => {
                leaves(left, out)?;
                leaves(right, out)
            }
            TableRef::JoinUsing { .. } | TableRef::Lateral(_) => Err(SqlError::Unsupported {
                what: "a LATERAL or USING join in a MERGE source".into(),
            }),
            t => {
                out.push(t);
                Ok(())
            }
        }
    }
    if !matches!(source, TableRef::Join { .. }) {
        return Ok(None);
    }
    let mut parts = Vec::new();
    leaves(source, &mut parts)?;
    let mut names = Vec::new();
    for leaf in parts {
        let alias = match leaf {
            TableRef::Table { name, alias, .. } => alias
                .as_ref()
                .map_or_else(|| name.name.value.clone(), |a| a.value.clone()),
            TableRef::Subquery { alias, .. }
            | TableRef::Function { alias, .. }
            | TableRef::Recursive { alias, .. }
            | TableRef::DataChange { alias, .. } => alias.value.clone(),
            _ => String::new(),
        };
        let mut q = Query::of(QueryBody::Select(Box::new(Select {
            distinct: false,
            items: vec![SelectItem::Star],
            from: vec![leaf.clone()],
            where_clause: None,
            group_by: Vec::new(),
            having: None,
        })));
        q.fetch_first = Some(crate::ast::Expr::num("0"));
        let rs = run_query(txn, ctx, params, &q, &Env::default())?;
        names.extend(std::iter::repeat_n(alias, rs.columns.len()));
    }
    Ok(Some(names))
}
