// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE SCHEMA` and `DROP SCHEMA`. A schema is a library with the
//! journal `QSQJRN` and its first receiver `QSQJRN0001`, to which the
//! tables SQL makes in it are journaled, and the catalog views over its
//! own objects; a long schema name has a generated system name and is
//! in QSYS's name index.

use db::sqlinf::names::Space;
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::{SqlContext, names};
use crate::ast::{Ident, Naming};
use crate::error::{Result, SqlError};

/// The journal a schema keeps for its tables.
pub const JOURNAL: &str = "QSQJRN";
/// Its first receiver.
pub const RECEIVER: &str = "QSQJRN0001";

/// The catalog views a schema holds over its own objects, with the
/// column each filters on. BLOCKED(rel): IBM's SYSCHKCST, SYSCSTDEP,
/// SYSFIELDS, SYSPACKAGE, SYSTABLEDEP and the SYSTRIG* views are not
/// made in a schema yet; QSYS2 has those it answers.
pub const CATALOG_VIEWS: &[(&str, &str)] = &[
    ("SYSCOLUMNS", "TABLE_SCHEMA"),
    ("SYSCST", "CONSTRAINT_SCHEMA"),
    ("SYSCSTCOL", "CONSTRAINT_SCHEMA"),
    ("SYSINDEXES", "INDEX_SCHEMA"),
    ("SYSKEYCST", "CONSTRAINT_SCHEMA"),
    ("SYSKEYS", "INDEX_SCHEMA"),
    ("SYSREFCST", "CONSTRAINT_SCHEMA"),
    ("SYSTABLES", "TABLE_SCHEMA"),
    ("SYSVIEWDEP", "VIEW_SCHEMA"),
    ("SYSVIEWS", "TABLE_SCHEMA"),
];

/// `CREATE SCHEMA`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &Ident) -> Result<()> {
    names::check_length(name)?;
    if names::schema(txn, ctx.qsys, name)?.is_some() {
        return Err(SqlError::Exists { name: name.value.clone(), kind: "*LIB" });
    }
    let (sys, sql) = match names::direct(name) {
        Some(n) => (n, None),
        None => (names::generate(txn, ctx.qsys, types::LIB, &name.value, &|_| false)?, Some(name.value.as_str())),
    };
    let now = ctx.clock();
    let lib = ops::create(
        txn,
        &store::obj::CreateParams {
            library: ctx.qsys,
            name: sys,
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: String::new(),
            system: String::new(),
            now,
            by_system: false,
        },
    )
    .map_err(|e| match e {
        store::StoreError::AlreadyExists { .. } => SqlError::Exists { name: name.value.clone(), kind: "*LIB" },
        e => e.into(),
    })?;
    let fixed = |n: &str| Name::parse(n).expect("a valid name");
    let rcv = jrn::rcv::crtjrnrcv(txn, lib, fixed(RECEIVER), &jrn::rcv::CrtJrnRcvOpts { now, ..jrn::rcv::CrtJrnRcvOpts::default() })
        .map_err(db::DbError::from)?;
    jrn::journal::crtjrn(txn, lib, fixed(JOURNAL), rcv, &jrn::journal::CrtJrnOpts { now, ..jrn::journal::CrtJrnOpts::default() })
        .map_err(db::DbError::from)?;
    if let Some(s) = sql {
        names::record_schema(txn, ctx.qsys, lib, s)?;
    }
    // The catalog views, made as SQL makes any view.
    let naming = ctx.naming;
    let shown = sql.map_or_else(|| sys.to_string(), str::to_string);
    let lib_ident = super::view::ident_of(sys);
    for (view, col) in CATALOG_VIEWS {
        let text = format!(
            "CREATE VIEW {lib_ident}.{view} AS SELECT * FROM QSYS2.{view} WHERE {col} = '{}'",
            shown.replace('\'', "''")
        );
        let crate::ast::Statement::CreateView(v) = crate::parse::parse(&text, Naming::Sql)? else {
            unreachable!("a CREATE VIEW");
        };
        ctx.naming = Naming::Sql;
        let r = super::view::create(txn, ctx, &v);
        ctx.naming = naming;
        r?;
    }
    Ok(())
}

/// Whether an object of a schema is one `CREATE SCHEMA` made.
fn made_with(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    let Ok((_, h)) = ops::read_header(txn, id) else {
        return false;
    };
    let n = h.name.to_string();
    match id.type_code {
        jrn::JRN => n == JOURNAL,
        jrn::JRNRCV => n.starts_with(JOURNAL),
        types::FILE => CATALOG_VIEWS.iter().any(|(v, _)| *v == n) && super::view::is_view(txn, id),
        _ => false,
    }
}

/// `DROP SCHEMA`: the library and everything in it. With `RESTRICT`
/// only a schema holding nothing but what `CREATE SCHEMA` put there.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &Ident, restrict: bool) -> Result<()> {
    let lib = names::schema(txn, ctx.qsys, name)?.ok_or_else(|| SqlError::ObjectNotFound { name: name.value.clone(), kind: "schema" })?;
    let objects: Vec<ObjectId> = ops::list(txn, lib, None)?.into_iter().map(|(_, id)| id).collect();
    if restrict {
        let mut others = Vec::new();
        for &id in &objects {
            if !made_with(txn, id) {
                others.push(ops::read_header(txn, id)?.1.name.to_string());
            }
        }
        if !others.is_empty() {
            return Err(SqlError::Restrict { name: name.value.clone(), what: others.join(", ") });
        }
    }
    let now = ctx.clock();
    // Files first, what depends on them with them; then the rest, the
    // journals before their receivers.
    for &id in objects.iter().filter(|id| id.type_code == types::FILE) {
        if ops::read_header(txn, id).is_err() {
            continue;
        }
        if db::file::describe(txn, id).is_ok_and(|d| !d.attr.journal.is_none()) {
            db::journal::endjrnpf(txn, ctx.job, id, now)?;
        }
        super::drop::drop_with_dependents(txn, ctx, id)?;
    }
    for &id in objects.iter().filter(|id| id.type_code == jrn::JRN) {
        jrn::journal::dltjrn(txn, ctx.job, id, now).map_err(db::DbError::from)?;
    }
    for &id in objects.iter().filter(|id| id.type_code == jrn::JRNRCV) {
        jrn::rcv::dltjrnrcv(txn, ctx.job, id).map_err(db::DbError::from)?;
    }
    for &id in objects.iter().filter(|id| ![types::FILE, jrn::JRN, jrn::JRNRCV].contains(&id.type_code)) {
        if ops::read_header(txn, id).is_ok() {
            ops::delete(txn, ctx.job, id)?;
        }
    }
    if let Some(sql) = (names::direct(name).is_none()).then(|| name.value.clone()) {
        db::sqlinf::names::remove(txn, ctx.qsys, Space::Schema, &sql)?;
    }
    ops::delete(txn, ctx.job, lib)?;
    Ok(())
}
