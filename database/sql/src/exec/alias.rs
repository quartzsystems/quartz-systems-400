// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE ALIAS` and `DROP ALIAS`: an alias is a `*FILE` that names a
//! table and, if it likes, a member; a statement that names the alias
//! reads and changes what it names.

use db::file::{self, AliasDef, CreateFileOpts};
use qsname::LibSpec;
use store::obj::ObjectId;
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{Ident, ObjectName, Statement};
use crate::error::{Result, SqlError};

/// `CREATE ALIAS` or `DROP ALIAS`.
pub fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, st: &Statement) -> Result<()> {
    match st {
        Statement::CreateAlias { name, table, member, replace } => create(txn, ctx, name, table, member.as_ref(), *replace),
        Statement::DropAlias(name) => drop(txn, ctx, name),
        _ => unreachable!("an alias statement"),
    }
}

/// `CREATE [OR REPLACE] ALIAS name FOR table [(member)]`.
pub fn create(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    name: &ObjectName,
    table: &ObjectName,
    member: Option<&Ident>,
    replace: bool,
) -> Result<()> {
    let sys = SqlContext::sys_name(&name.name)?;
    let library = ctx.library_for(txn, name)?;
    // The table's library as the statement says it, the library list
    // when it says none: the alias is resolved each time it is used.
    let def = AliasDef {
        library: match ctx.lib_spec(table)? {
            LibSpec::Name(n) => Some(n),
            LibSpec::Curlib | LibSpec::Libl => None,
        },
        file: SqlContext::sys_name(&table.name)?,
        member: member.map(SqlContext::sys_name).transpose()?,
    };
    let lib = LibSpec::Name(library_name(txn, library)?);
    if replace && let Ok(found) = store::obj::ops::resolve(txn, ctx.qsys, &ctx.names, lib, sys, store::obj::types::FILE) {
        if file::alias_of(txn, found.id)?.is_none() {
            return Err(SqlError::Database { what: format!("{sys} exists and is not an alias"), msgid: None });
        }
        file::delete_alias(txn, ctx.job, found.id)?;
    }
    file::create_alias(
        txn,
        library,
        sys,
        &def,
        &CreateFileOpts {
            now: ctx.clock(),
            text: String::new(),
            ..CreateFileOpts::default()
        },
    )?;
    Ok(())
}

fn library_name(txn: &mut Txn<'_>, lib: ObjectId) -> Result<qsname::Name> {
    Ok(store::obj::ops::read_header(txn, lib)?.1.name)
}

/// `DROP ALIAS`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<()> {
    let id = ctx.resolve_file(txn, name)?;
    if file::alias_of(txn, id)?.is_none() {
        return Err(SqlError::ObjectNotFound { name: name.to_string(), kind: "alias" });
    }
    file::delete_alias(txn, ctx.job, id)?;
    Ok(())
}

/// The table an alias names, as an object name the context resolves.
pub fn target(def: &AliasDef) -> ObjectName {
    let file = def.file.to_string();
    match def.library {
        Some(l) => ObjectName::qualified(&l.to_string(), &file),
        None => ObjectName::new(&file),
    }
}
