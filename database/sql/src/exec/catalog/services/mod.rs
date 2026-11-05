// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` and `SYSTOOLS` services: views, table functions, scalar
//! functions and procedures, with pub400's columns ([`shape`]) and
//! values from the store or the [`host::SystemHost`]. A column QS/400
//! has no value for is null; each service lists the columns of the
//! oracle's that are `NOT NULL` and still have none here (`NO_VALUE`),
//! and those are nullable in its description.

pub mod host;
pub mod http;
pub mod shape;

mod args;
mod out;

mod joblog;
mod jobs;
mod libl;
mod netstat;
mod objects;
mod products;
mod spool;
mod sysval;
mod users;

pub use args::Args;
pub use out::Out;

use store::txn::Txn;

use super::super::SqlContext;
use super::Table;
use crate::ast::ObjectName;
use crate::error::{Result, SqlError};

/// A view's rows.
type ViewFn = fn(&mut Txn<'_>, &SqlContext, &mut Out) -> Result<()>;
/// A table function's rows for its arguments.
type TableFn = fn(&mut Txn<'_>, &SqlContext, &Args, &mut Out) -> Result<()>;

/// A view: schema, name, the oracle's `NOT NULL` columns with no value
/// here, its rows.
struct View {
    schema: &'static str,
    name: &'static str,
    no_value: &'static [&'static str],
    rows: ViewFn,
}

/// A table function, by specific name.
struct Function {
    schema: &'static str,
    specific: &'static str,
    no_value: &'static [&'static str],
    rows: TableFn,
}

/// The views.
const VIEWS: &[View] = &[
    View { schema: "QSYS2", name: "LIBRARY_LIST_INFO", no_value: &[], rows: libl::rows },
    View { schema: "QSYS2", name: "SYSTEM_VALUE_INFO", no_value: &[], rows: sysval::rows },
    View { schema: "QSYS2", name: "SYSTEM_STATUS_INFO", no_value: jobs::STATUS_NO_VALUE, rows: jobs::status },
    View { schema: "QSYS2", name: "SYSTEM_STATUS_INFO_BASIC", no_value: jobs::STATUS_NO_VALUE, rows: jobs::status },
    View { schema: "QSYS2", name: "USER_INFO", no_value: users::NO_VALUE, rows: users::info },
    View { schema: "QSYS2", name: "USER_INFO_BASIC", no_value: users::NO_VALUE, rows: users::info },
    View { schema: "QSYS2", name: "GROUP_PROFILE_ENTRIES", no_value: &[], rows: users::groups },
    View { schema: "QSYS2", name: "OUTPUT_QUEUE_ENTRIES", no_value: spool::ENTRY_NO_VALUE, rows: spool::entries },
    View { schema: "QSYS2", name: "OUTPUT_QUEUE_ENTRIES_BASIC", no_value: spool::ENTRY_NO_VALUE, rows: spool::entries },
    View { schema: "QSYS2", name: "OUTPUT_QUEUE_INFO", no_value: spool::QUEUE_NO_VALUE, rows: spool::queues },
    View { schema: "QSYS2", name: "SOFTWARE_PRODUCT_INFO", no_value: products::PRODUCT_NO_VALUE, rows: products::products },
    View { schema: "QSYS2", name: "PTF_INFO", no_value: &[], rows: products::ptfs },
    View { schema: "QSYS2", name: "NETSTAT_INFO", no_value: netstat::INFO_NO_VALUE, rows: netstat::info },
    View { schema: "QSYS2", name: "NETSTAT_JOB_INFO", no_value: &[], rows: netstat::jobs },
    View { schema: "QSYS2", name: "NETSTAT_INTERFACE_INFO", no_value: &[], rows: netstat::interfaces },
    View { schema: "QSYS2", name: "NETSTAT_ROUTE_INFO", no_value: netstat::ROUTE_NO_VALUE, rows: netstat::routes },
];

/// The table functions.
const FUNCTIONS: &[Function] = &[
    Function { schema: "QSYS2", specific: "QSQACTJOB", no_value: &[], rows: jobs::active },
    Function { schema: "QSYS2", specific: "QSQJOBINFO", no_value: &[], rows: jobs::job_info },
    Function { schema: "QSYS2", specific: "QSQOBSTAT", no_value: &[], rows: objects::rows },
    Function { schema: "QSYS2", specific: "QSQJOBLOG", no_value: &[], rows: joblog::rows },
    Function { schema: "SYSTOOLS", specific: "SPOOL_FILE", no_value: &[], rows: spool::data },
    Function { schema: "QSYS2", specific: "HTTP_GETV", no_value: &[], rows: http::get_v },
    Function { schema: "QSYS2", specific: "HTTP_GETBV", no_value: &[], rows: http::get_bv },
    Function { schema: "QSYS2", specific: "HTTP_POSTV", no_value: &[], rows: http::post_v },
    Function { schema: "QSYS2", specific: "HTTP_POSTBV", no_value: &[], rows: http::post_bv },
    Function { schema: "QSYS2", specific: "HTTP_PUTV", no_value: &[], rows: http::put_v },
    Function { schema: "QSYS2", specific: "HTTP_PUTBV", no_value: &[], rows: http::put_bv },
    Function { schema: "QSYS2", specific: "HTTP_PATCHV", no_value: &[], rows: http::patch_v },
    Function { schema: "QSYS2", specific: "HTTP_PTCBV", no_value: &[], rows: http::patch_bv },
    Function { schema: "QSYS2", specific: "HTTP_DELV", no_value: &[], rows: http::delete_v },
    Function { schema: "QSYS2", specific: "HTTP_DELBV", no_value: &[], rows: http::delete_bv },
];

fn schema_of(name: &ObjectName) -> Option<&str> {
    name.schema.as_ref().map(|s| s.value.as_str())
}

fn view_of(name: &ObjectName) -> Option<&'static View> {
    VIEWS
        .iter()
        .find(|v| v.name == name.name.value && schema_of(name).is_none_or(|s| s == v.schema))
}

/// Whether `name` is one of the service views.
pub fn has(name: &ObjectName) -> bool {
    view_of(name).is_some()
}

/// A service view's columns and rows.
pub fn table(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName, alias: &str) -> Result<Option<Table>> {
    let Some(v) = view_of(name) else {
        return Ok(None);
    };
    let shape = shape::view(v.schema, v.name).ok_or_else(|| missing(v.name))?;
    let mut out = Out::new(shape, alias, v.no_value);
    (v.rows)(txn, ctx, &mut out)?;
    Ok(Some(out.finish()))
}

/// A service view's columns only, its rows not computed: what a
/// `DESCRIBE` needs.
#[must_use]
pub fn columns(name: &ObjectName, alias: &str) -> Option<Table> {
    let v = view_of(name)?;
    let shape = shape::view(v.schema, v.name)?;
    Some(Out::new(shape, alias, v.no_value).finish())
}

fn missing(name: &str) -> SqlError {
    SqlError::Database { what: format!("the shape of {name} is not in the shapes table"), msgid: None }
}

/// A table function's columns and rows for `args` (positional, or
/// `name => value`), `None` when `name` is not a service table
/// function.
pub fn table_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: Vec<(Option<String>, crate::value::Value)>,
    alias: &str,
) -> Result<Option<Table>> {
    let schema = schema_of(name);
    for f in FUNCTIONS {
        if schema.is_some_and(|s| s != f.schema) {
            continue;
        }
        let Some(shape) = shape::routines(f.schema, &name.name.value).iter().find(|s| s.specific == f.specific) else {
            continue;
        };
        let bound = Args::bind(shape, args, ctx)?;
        let mut out = Out::new(shape, alias, f.no_value);
        (f.rows)(txn, ctx, &bound, &mut out)?;
        return Ok(Some(out.finish()));
    }
    Ok(None)
}

/// The `TABLE (f(args))` hook ([`super::FunctionProvider`]): the
/// arguments' names from their `name => value` markers.
pub(crate) fn call(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: &[crate::value::Value],
    exprs: &[crate::ast::Expr],
    alias: &str,
) -> Result<Option<Table>> {
    let named = args
        .iter()
        .zip(exprs.iter().map(Some).chain(std::iter::repeat(None)))
        .map(|(v, e)| {
            let n = match e {
                Some(crate::ast::Expr::Function(f)) => f.name.strip_prefix("=>").map(str::to_string),
                _ => None,
            };
            (n, v.clone())
        })
        .collect();
    table_function(txn, ctx, name, named, alias)
}

/// Whether `name` is a service table function.
#[must_use]
pub fn is_table_function(name: &ObjectName) -> bool {
    let schema = schema_of(name);
    FUNCTIONS.iter().any(|f| {
        schema.is_none_or(|s| s == f.schema)
            && shape::routines(f.schema, &name.name.value).iter().any(|s| s.specific == f.specific)
    })
}

/// The host, or the error a service needing it raises: SQL0443 as IBM
/// raises a service's failure, with our text.
pub(crate) fn host(ctx: &SqlContext) -> Result<&dyn host::SystemHost> {
    ctx.host.as_deref().ok_or_else(|| unreachable_here("the system's state"))
}

/// SQL0443: a service could not get what it reads.
pub(crate) fn unreachable_here(what: &str) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what: format!("{what} cannot be read where this statement runs") }
}

/// Ask the host with the statement's transaction given back, so the
/// host may use what the store lends (the system values) meanwhile.
pub(crate) fn ask<T>(txn: &mut Txn<'_>, ctx: &SqlContext, what: &str, f: impl FnOnce(&dyn host::SystemHost) -> Option<T>) -> Result<T> {
    let h = host(ctx)?;
    let suspended = txn.suspend();
    let r = f(h);
    if suspended {
        txn.resume()?;
    }
    r.ok_or_else(|| unreachable_here(what))
}

/// Every service run once, its default arguments: the columns a
/// service set that its shape lacks (outside the `_BASIC` views) and the
/// `NOT NULL` columns it left null. For the tests, over a full host.
pub fn self_check(txn: &mut Txn<'_>, ctx: &SqlContext) -> Vec<String> {
    let mut faults = Vec::new();
    let mut note = |name: &str, r: Result<(Table, Vec<String>)>| match r {
        Ok(((schema, rows), unknown)) => {
            if !name.ends_with("_BASIC") {
                faults.extend(unknown.into_iter().map(|c| format!("{name}: no column {c}")));
            }
            for row in &rows {
                for (c, v) in schema.cols.iter().zip(row) {
                    if !c.nullable && v.is_null() {
                        faults.push(format!("{name}: NOT NULL column {} is null", c.name));
                    }
                }
            }
        }
        Err(e) => faults.push(format!("{name}: {e}")),
    };
    for v in VIEWS {
        let r = shape::view(v.schema, v.name).ok_or_else(|| missing(v.name)).and_then(|shape| {
            let mut out = Out::new(shape, v.name, v.no_value);
            (v.rows)(txn, ctx, &mut out)?;
            Ok(out.finish_noting())
        });
        note(v.name, r);
    }
    for f in FUNCTIONS {
        let r = shape::index_specific(f.schema, f.specific).ok_or_else(|| missing(f.specific)).and_then(|shape| {
            // A parameter with no default gets a sample: `*ALL` for a
            // type list, QSYS for a library, a path or URL for other
            // text, 1 for a number.
            let sample = shape
                .parms
                .iter()
                .filter(|p| p.mode != "OUT" && (p.default.is_empty() || p.default == "-"))
                .map(|p| {
                    let v = if matches!(p.ty.as_str(), "INTEGER" | "SMALLINT" | "BIGINT" | "DECIMAL") {
                        crate::value::Value::int(1)
                    } else if p.name == "JOB_NAME" {
                        crate::value::Value::Str("*".into())
                    } else if p.name.contains("TYPE") || p.name.contains("LIST") {
                        crate::value::Value::Str("*ALL".into())
                    } else if p.name.contains("SCHEMA") || p.name.contains("LIBRARY") {
                        crate::value::Value::Str("QSYS".into())
                    } else {
                        crate::value::Value::Str("/".into())
                    };
                    (Some(p.name.clone()), v)
                })
                .collect();
            let bound = Args::bind(shape, sample, ctx)?;
            let mut out = Out::new(shape, f.specific, f.no_value);
            (f.rows)(txn, ctx, &bound, &mut out)?;
            Ok(out.finish_noting())
        });
        note(f.specific, r);
    }
    faults.sort();
    faults.dedup();
    faults
}
