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
        Statement::CreateAlias {
            name,
            table,
            member,
            replace,
        } => create(txn, ctx, name, table, member.as_ref(), *replace),
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
    let library = ctx.library_for(txn, name)?;
    // The table's library as the statement says it, the library list
    // when it says none: the alias is resolved each time it is used. A
    // long table name is kept as the system name it has now.
    let file_name = if let Some(n) = super::names::direct(&table.name) {
        n
    } else {
        let id = ctx.resolve_file(txn, table)?;
        store::obj::ops::read_header(txn, id)?.1.name
    };
    let def = AliasDef {
        library: match ctx.lib_spec_in(txn, table)? {
            LibSpec::Name(n) => Some(n),
            LibSpec::Curlib | LibSpec::Libl => None,
        },
        file: file_name,
        member: member.map(SqlContext::sys_name).transpose()?,
    };
    let qualified = ObjectName {
        schema: Some(super::view::ident_of(library_name(txn, library)?)),
        name: name.name.clone(),
    };
    if let Ok(found) = ctx.resolve_file(txn, &qualified) {
        if !replace || file::alias_of(txn, found)?.is_none() {
            return Err(SqlError::Exists {
                name: name.name.value.clone(),
                kind: "*FILE",
            });
        }
        file::delete_file(txn, ctx.job, found)?;
    }
    let (sys, sql) = super::names::new_object(txn, library, &name.name, None)?;
    let id = file::create_alias(
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
    super::names::record(
        txn,
        library,
        id,
        db::sqlinf::Kind::Alias,
        sql.as_deref(),
        ctx.clock(),
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
        return Err(SqlError::ObjectNotFound {
            name: name.to_string(),
            kind: "alias",
        });
    }
    file::delete_file(txn, ctx.job, id)?;
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
