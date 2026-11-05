// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DROP TABLE`, `DROP VIEW`, `DROP INDEX`: the file and what depends on
//! it (the logical files over it and the views whose queries read it),
//! refused with `RESTRICT` when anything does (`SQL0616`).

use db::file;
use store::obj::ObjectId;
use store::txn::Txn;

use super::{SqlContext, names, view};
use crate::ast::ObjectName;
use crate::error::{Result, SqlError};

/// The objects that depend on file `id`: the logical files over it and
/// the views that read it.
pub fn dependents(txn: &mut Txn<'_>, ctx: &SqlContext, id: ObjectId) -> Result<Vec<ObjectId>> {
    let desc = file::describe(txn, id)?;
    let mut out: Vec<ObjectId> = desc.dependents.iter().map(|d| d.lf_file).collect();
    out.extend(view::dependents(txn, ctx, id)?);
    out.sort();
    out.dedup();
    out.retain(|&d| d != id);
    Ok(out)
}

/// Drop file `id` and, first, everything that depends on it.
pub fn drop_with_dependents(txn: &mut Txn<'_>, ctx: &mut SqlContext, id: ObjectId) -> Result<()> {
    for d in dependents(txn, ctx, id)? {
        if store::obj::ops::read_header(txn, d).is_ok() {
            drop_with_dependents(txn, ctx, d)?;
        }
    }
    // Journaling ends as the table goes, as IBM's delete ends it.
    if file::describe(txn, id).is_ok_and(|d| !d.attr.journal.is_none()) {
        db::journal::endjrnpf(txn, ctx.job, id, ctx.clock())?;
    }
    file::delete_file(txn, ctx.job, id)?;
    Ok(())
}

/// `DROP TABLE`, `DROP VIEW` or `DROP INDEX`. Without `RESTRICT` what
/// depends on the object goes with it, as with `CASCADE`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName, restrict: bool) -> Result<()> {
    let id = ctx.resolve_file(txn, name)?;
    if restrict {
        let deps = dependents(txn, ctx, id)?;
        if !deps.is_empty() {
            let what: Vec<String> = deps.iter().map(|&d| names::sql_name(txn, d)).collect();
            return Err(SqlError::Restrict { name: name.name.value.clone(), what: what.join(", ") });
        }
    }
    drop_with_dependents(txn, ctx, id)
}
