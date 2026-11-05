// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Distinct types: `CREATE TYPE ... AS` a source type makes a `*SQLUDT`
//! holding the source in its `TYPE` entry (`docs/DB_FORMAT.md` §8.1,
//! `SqlType`); `DROP TYPE` deletes it; `SYSTYPES` lists them.
//! BLOCKED(sql): using a distinct type (a column, a variable, `CAST`, the
//! generated cast functions) needs strong typing through the engine and
//! is refused with `SQL0084`.

use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::var::Udt;
use db::sqlinf::{self, Kind, SqlInfo, tag};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{CreateType, ObjectName};
use crate::error::{Result, SqlError};

fn err(code: i32, state: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Coded { code, state, what: what.into() }
}

/// The type `name` names: its `*SQLUDT`.
pub fn find(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<Option<(ObjectId, Udt)>> {
    let Ok(lib) = ctx.library_for(txn, name) else {
        return Ok(None);
    };
    let id = if let Some(t) = names::find(txn, lib, Space::Type, &name.name.value)? {
        t.id
    } else {
        let Ok(sys) = SqlContext::sys_name(&name.name) else {
            return Ok(None);
        };
        let l = store::obj::library::Library::load(txn, lib)?;
        let Some(id) = l.lookup(txn, types::SQLUDT, sys)? else {
            return Ok(None);
        };
        id
    };
    let Some(info) = sqlinf::load(txn, id)?.filter(|i| i.kind == Kind::Type) else {
        return Ok(None);
    };
    let Some(body) = info.first(tag::TYPE) else {
        return Ok(None);
    };
    Ok(Some((id, Udt::decode(body)?)))
}

/// `CREATE TYPE`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, c: &CreateType) -> Result<()> {
    super::cast::check_column(&c.source)?;
    let lib = ctx.library_for(txn, &c.name)?;
    if find(txn, ctx, &c.name)?.is_some() {
        return Err(err(-601, "42710", format!("type {} exists", c.name.name.value)));
    }
    let long = c.name.name.value.len() > 10;
    let sys = if long {
        super::names::generate(txn, lib, types::SQLUDT, &c.name.name.value, &|_| false)?
    } else {
        SqlContext::sys_name(&c.name.name)?
    };
    let now = ctx.clock();
    let id = ops::create(
        txn,
        &store::obj::CreateParams {
            library: lib,
            name: sys,
            type_code: types::SQLUDT,
            attribute: String::new(),
            text: String::new(),
            owner: ctx.user.clone(),
            system: "QS400".into(),
            now,
            by_system: false,
        },
    )?;
    let mut info = SqlInfo::new(Kind::Type);
    if long {
        info.set(tag::NAME, c.name.name.value.as_bytes().to_vec());
    }
    info.set(tag::TYPE, Udt { metatype: b'T', source: crate::pl::types::type_text(&c.source) }.encode());
    sqlinf::save(txn, id, &info, now)?;
    names::put(txn, lib, Space::Type, &c.name.name.value, Target { id, system: sys })?;
    Ok(())
}

/// `DROP TYPE`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<()> {
    let (id, _) = find(txn, ctx, name)?.ok_or_else(|| SqlError::ObjectNotFound { kind: "type", name: name.name.value.clone() })?;
    db::auth::check(txn, "obj.delete", id).map_err(SqlError::from)?;
    let lib = ctx.library_for(txn, name)?;
    names::remove(txn, lib, Space::Type, &name.name.value)?;
    ops::delete(txn, ctx.job, id)?;
    Ok(())
}

/// A type listed: system name, SQL name, the type, created, owner.
pub type Listed = (Name, String, Udt, u64, String);

/// The distinct types of a library: system name, SQL name, the type,
/// created, owner.
pub fn in_library(txn: &mut Txn<'_>, lib: ObjectId) -> Result<Vec<Listed>> {
    let mut out = Vec::new();
    for (sys, id) in ops::list(txn, lib, Some(types::SQLUDT))? {
        let Ok(Some(info)) = sqlinf::load(txn, id) else { continue };
        let Some(Ok(u)) = info.first(tag::TYPE).map(Udt::decode) else { continue };
        let (_, h) = ops::read_header(txn, id)?;
        let name = info.name().unwrap_or_else(|| sys.name.to_string());
        out.push((sys.name, name, u, h.created, ccsid::decode_trimmed(&h.owner)));
    }
    Ok(out)
}
