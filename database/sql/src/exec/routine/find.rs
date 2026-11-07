// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding routines: a library's routines are the specific names in its
//! SQL name index, each naming the object whose `*SQLINF` holds it.

use db::sqlinf::names::{self, Space};
use db::sqlinf::routine::Routine;
use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ast::{Naming, ObjectName};
use crate::error::Result;
use crate::exec::SqlContext;
use crate::exec::names as sqlnames;

/// A routine and where it is.
#[derive(Clone, Debug)]
pub struct Found {
    /// Its library.
    pub lib: ObjectId,
    /// The library's SQL schema name.
    pub schema: String,
    /// The object holding its `*SQLINF`.
    pub owner: ObjectId,
    /// The routine.
    pub routine: Routine,
}

/// Every routine of a library.
pub fn in_library(txn: &mut Txn<'_>, lib: ObjectId) -> Result<Vec<Found>> {
    let schema = sqlnames::schema_name(txn, lib);
    let mut out = Vec::new();
    for (specific, t) in names::list(txn, lib, Space::Specific)? {
        if ops::read_header(txn, t.id).is_err() {
            continue;
        }
        let Some(info) = db::sqlinf::load(txn, t.id)? else {
            continue;
        };
        for r in Routine::all(&info)? {
            if r.def.specific == specific {
                out.push(Found {
                    lib,
                    schema: schema.clone(),
                    owner: t.id,
                    routine: r,
                });
            }
        }
    }
    Ok(out)
}

/// The libraries an unqualified routine name is looked for in: the
/// SQL path (`QSYS`, `QSYS2`, `SYSPROC`, `SYSIBMADM`, then the current
/// schema) under `*SQL` naming, the library list under `*SYS`.
pub fn path(txn: &mut Txn<'_>, ctx: &SqlContext) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let add = |id: ObjectId, out: &mut Vec<ObjectId>| {
        if !out.contains(&id) {
            out.push(id);
        }
    };
    match ctx.naming {
        Naming::Sql => {
            for n in path_names(ctx) {
                if let Ok(name) = Name::parse(&n)
                    && let Some(id) = library(txn, ctx, name)
                {
                    add(id, &mut out);
                }
            }
        }
        Naming::Sys => {
            if let Some(c) = ctx.names.curlib {
                add(c, &mut out);
            }
            for &l in &ctx.names.libl {
                add(l, &mut out);
            }
        }
    }
    out
}

/// The SQL path's schema names (`SYSROUTINES.SQL_PATH`).
#[must_use]
pub fn path_names(ctx: &SqlContext) -> Vec<String> {
    let mut v: Vec<String> = ["QSYS", "QSYS2", "SYSPROC", "SYSIBMADM"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let own = ctx
        .current_schema
        .map_or_else(|| ctx.user.clone(), |n| n.to_string());
    if !v.contains(&own) {
        v.push(own);
    }
    v
}

fn library(txn: &mut Txn<'_>, ctx: &SqlContext, name: Name) -> Option<ObjectId> {
    let q = Library::load(txn, ctx.qsys).ok()?;
    q.lookup(txn, types::LIB, name).ok().flatten()
}

/// The libraries a routine name means: its schema, or the path.
pub fn libraries(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<Vec<ObjectId>> {
    match &name.schema {
        Some(s) => Ok(vec![ctx.schema_library(txn, s)?]),
        None => Ok(path(txn, ctx)),
    }
}

/// The routines of a name and type (1 procedure, 2 function), in the
/// first library of the path that has any.
pub fn named(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    name: &ObjectName,
    routine_type: u8,
) -> Result<Vec<Found>> {
    for lib in libraries(txn, ctx, name)? {
        let found: Vec<Found> = in_library(txn, lib)?
            .into_iter()
            .filter(|f| {
                f.routine.def.routine_type == routine_type && f.routine.def.name == name.name.value
            })
            .collect();
        if !found.is_empty() {
            return Ok(found);
        }
    }
    Ok(Vec::new())
}

/// The routine of a specific name.
pub fn specific(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<Option<Found>> {
    for lib in libraries(txn, ctx, name)? {
        if let Some(f) = in_library(txn, lib)?
            .into_iter()
            .find(|f| f.routine.def.specific == name.name.value)
        {
            return Ok(Some(f));
        }
    }
    Ok(None)
}
