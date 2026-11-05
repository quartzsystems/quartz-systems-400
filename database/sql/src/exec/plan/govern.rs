// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The predictive query governor: before a query runs, its run time and
//! temporary storage are estimated and held against the job's query
//! options `QUERY_TIME_LIMIT` (seconds) and `QUERY_STORAGE_LIMIT` or
//! `STORAGE_LIMIT` (MiB). Past either, the query is not run: SQL0666,
//! SQLSTATE `57005`, as on IBM i. Run time is reckoned from the rows of
//! the tables it reads, at a million rows a second and never under one
//! second (pub400 estimates a one-row query at 1, so a limit of 0 stops
//! every query); storage from the rows it must keep to sort, group or
//! join.
//!
//! The statement's working storage is limited apart from this
//! (`mem`): passing that ends a running statement with SQL0904.

use db::member::Member;
use store::txn::Txn;

use super::super::SqlContext;
use crate::ast::{Query, QueryBody, TableRef};
use crate::error::{Result, SqlError};

/// Rows a second the estimate reckons with.
const ROWS_PER_SECOND: u64 = 1_000_000;

/// The rows `q` reads and whether it keeps them (sorts, groups, joins);
/// with the bytes of a row it reads, near enough.
fn reads(txn: &mut Txn<'_>, ctx: &SqlContext, q: &Query) -> (u64, bool, u64) {
    let mut rows = 0;
    let mut keeps = !q.order_by.is_empty();
    let mut width = 0;
    body(txn, ctx, &q.body, &mut rows, &mut keeps, &mut width);
    (rows, keeps, width)
}

fn body(txn: &mut Txn<'_>, ctx: &SqlContext, b: &QueryBody, rows: &mut u64, keeps: &mut bool, width: &mut u64) {
    match b {
        QueryBody::Select(s) => {
            *keeps |= s.distinct || !s.group_by.is_empty() || s.from.len() > 1;
            for t in &s.from {
                table(txn, ctx, t, rows, keeps, width);
            }
        }
        QueryBody::Union { left, right, all } => {
            *keeps |= !all;
            body(txn, ctx, left, rows, keeps, width);
            body(txn, ctx, right, rows, keeps, width);
        }
        QueryBody::Nested(q) => {
            *keeps |= !q.order_by.is_empty();
            body(txn, ctx, &q.body, rows, keeps, width);
        }
        QueryBody::Values(v) => *rows += v.len() as u64,
    }
}

fn table(txn: &mut Txn<'_>, ctx: &SqlContext, t: &TableRef, rows: &mut u64, keeps: &mut bool, width: &mut u64) {
    match t {
        TableRef::Table { name, .. } => {
            if let Ok(info) = ctx.resolve_table(txn, name) {
                let n = Member::open(txn, &info.desc, &info.member).map_or(0, |m| m.attr().record_count);
                *rows = rows.saturating_add(n);
                *width = (*width).max(u64::from(info.desc.attr.record_len));
            }
        }
        TableRef::Subquery { query, .. } => {
            let (r, k, w) = reads(txn, ctx, query);
            *rows = rows.saturating_add(r);
            *keeps |= k;
            *width = (*width).max(w);
        }
        TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
            *keeps = true;
            table(txn, ctx, left, rows, keeps, width);
            table(txn, ctx, right, rows, keeps, width);
        }
        TableRef::Lateral(inner) => table(txn, ctx, inner, rows, keeps, width),
        _ => {}
    }
}

fn limit(ctx: &SqlContext, name: &str) -> Option<u64> {
    let v = ctx.qaqqini.get(name);
    if v == "*DEFAULT" || v == "*NOMAX" {
        None
    } else {
        v.parse().ok()
    }
}

/// Hold `q` against the job's limits before it runs.
pub fn check(txn: &mut Txn<'_>, ctx: &SqlContext, q: &Query) -> Result<()> {
    let time = limit(ctx, "QUERY_TIME_LIMIT");
    let storage = limit(ctx, "QUERY_STORAGE_LIMIT").or_else(|| limit(ctx, "STORAGE_LIMIT"));
    if time.is_none() && storage.is_none() {
        return Ok(());
    }
    let (rows, keeps, width) = reads(txn, ctx, q);
    let seconds = rows.div_ceil(ROWS_PER_SECOND).max(1);
    if let Some(t) = time
        && seconds > t
    {
        return Err(SqlError::Coded {
            code: -666,
            state: "57005",
            what: format!("the query's estimated run time of {seconds} seconds passes QUERY_TIME_LIMIT {t}"),
        });
    }
    let mib = if keeps { rows.saturating_mul(width.max(1)) >> 20 } else { 0 };
    if let Some(s) = storage
        && mib > s
    {
        return Err(SqlError::Coded {
            code: -666,
            state: "57005",
            what: format!("the query's estimated temporary storage of {mib} MiB passes QUERY_STORAGE_LIMIT {s}"),
        });
    }
    Ok(())
}
