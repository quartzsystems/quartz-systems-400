// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Catalog views and services computed when a query names them. Each
//! provider answers for its own names; [`PROVIDERS`] lists them in the
//! order they are asked.

pub mod auth;
pub mod cst;
pub mod routines;
pub mod sys;
pub mod systools;

use store::txn::Txn;

use super::SqlContext;
use super::eval::{Row, Schema};
use crate::ast::ObjectName;
use crate::error::Result;

/// A computed view's columns and rows.
pub(crate) type Table = (Schema, Vec<Row>);

/// One provider of computed views.
pub(crate) struct Provider {
    /// Whether `name` is one of its views.
    pub has: fn(&ObjectName) -> bool,
    /// Its columns and rows for `name` (correlation `alias`), `None` when
    /// it is not one of its views.
    pub table: fn(&mut Txn<'_>, &SqlContext, &ObjectName, &str) -> Result<Option<Table>>,
}

/// The providers.
const PROVIDERS: &[Provider] = &[
    Provider { has: sys::has, table: sys::virtual_table },
    Provider { has: auth::has, table: auth::virtual_table },
    Provider { has: cst::has, table: cst::table },
    Provider { has: routines::has, table: routines::virtual_table },
];

/// Whether `name` names a computed view.
#[must_use]
pub fn is_view(name: &ObjectName) -> bool {
    PROVIDERS.iter().any(|p| (p.has)(name))
}

/// The rows and columns of a computed view, or `None` when `name` is not
/// one.
pub fn virtual_table(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    alias: &str,
) -> Result<Option<Table>> {
    for p in PROVIDERS {
        if (p.has)(name)
            && let Some(t) = (p.table)(txn, ctx, name, alias)?
        {
            return Ok(Some(t));
        }
    }
    Ok(None)
}

/// A table function call: the function, its arguments' values and
/// expressions, the correlation name.
pub(crate) type FunctionCall =
    fn(&mut Txn<'_>, &SqlContext, &ObjectName, &[crate::value::Value], &[crate::ast::Expr], &str) -> Result<Option<Table>>;

/// One provider of table functions: `TABLE (name(args)) AS x`.
pub(crate) struct FunctionProvider {
    /// Whether `name` is one of its functions.
    pub has: fn(&ObjectName) -> bool,
    /// The columns and rows of `name` called with `args` (correlation
    /// `alias`); `None` when it is not one of its functions.
    pub call: FunctionCall,
}

/// The table function providers, asked in order.
const FUNCTIONS: &[FunctionProvider] = &[
    FunctionProvider { has: systools::has, call: systools::call },
    FunctionProvider { has: super::json::table::has, call: super::json::table::call },
];

/// Whether `name` names a table function.
#[must_use]
pub fn is_table_function(name: &ObjectName) -> bool {
    FUNCTIONS.iter().any(|p| (p.has)(name))
}

/// The rows and columns of a table function call, or `None` when `name`
/// is not one.
pub fn table_function(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    args: &[crate::value::Value],
    exprs: &[crate::ast::Expr],
    alias: &str,
) -> Result<Option<Table>> {
    for p in FUNCTIONS {
        if (p.has)(name)
            && let Some(t) = (p.call)(txn, ctx, name, args, exprs, alias)?
        {
            return Ok(Some(t));
        }
    }
    Ok(None)
}
