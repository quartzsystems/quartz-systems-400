// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Global variables: `CREATE VARIABLE` makes a `*SRVPGM` holding the
//! variable's definition in its `VARIABLE` entry (`docs/DB_FORMAT.md`
//! §8.1, `SqlVariable`); each job's value lives in its SQL context, made
//! from the default when the job first reads the variable. The rules are
//! the primary's (`tools/conformance/sql/cases/objects2/VARIABLE.sql`).

use std::collections::HashMap;

use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::var::Variable;
use db::sqlinf::{self, Kind, SqlInfo, tag};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::SqlContext;
use super::eval::{Env, Evaluator};
use crate::ast::{CreateVariable, Expr, Ident, ObjectName, TypeSpec};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// A job's values of its global variables, by object.
#[derive(Clone, Debug, Default)]
pub struct Values {
    set: HashMap<ObjectId, Value>,
}

/// A variable found.
#[derive(Clone, Debug)]
pub struct Found {
    /// Its `*SRVPGM`.
    pub id: ObjectId,
    /// Its definition.
    pub def: Variable,
    /// Its type.
    pub data_type: TypeSpec,
}

fn err(code: i32, state: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Coded {
        code,
        state,
        what: what.into(),
    }
}

/// The variable `name` names, if there is one: qualified, in its
/// schema; unqualified, in the default schema.
pub fn find(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<Option<Found>> {
    let Ok(lib) = ctx.library_for(txn, name) else {
        return Ok(None);
    };
    let id = if let Some(t) = names::find(txn, lib, Space::Variable, &name.name.value)? {
        t.id
    } else {
        let Ok(sys) = SqlContext::sys_name(&name.name) else {
            return Ok(None);
        };
        let l = store::obj::library::Library::load(txn, lib)?;
        let Some(id) = l.lookup(txn, types::SRVPGM, sys)? else {
            return Ok(None);
        };
        id
    };
    let Some(info) = sqlinf::load(txn, id)?.filter(|i| i.kind == Kind::Variable) else {
        return Ok(None);
    };
    let Some(body) = info.first(tag::VARIABLE) else {
        return Ok(None);
    };
    let def = Variable::decode(body)?;
    let data_type = crate::pl::types::parse_type(&def.data_type)?;
    Ok(Some(Found { id, def, data_type }))
}

fn not_found(name: &ObjectName) -> SqlError {
    let n = match &name.schema {
        Some(s) => format!("{}.{}", s.value, name.name.value),
        None => name.name.value.clone(),
    };
    SqlError::ColumnNotFound { name: n }
}

/// The value of a variable for this job: the one set, else its default
/// (null when it has none), kept from now on.
pub fn value(txn: &mut Txn<'_>, ctx: &mut SqlContext, f: &Found) -> Result<Value> {
    if let Some(v) = ctx.vars.set.get(&f.id) {
        return Ok(v.clone());
    }
    let v = default_of(txn, ctx, f)?;
    ctx.vars.set.insert(f.id, v.clone());
    Ok(v)
}

fn default_of(txn: &mut Txn<'_>, ctx: &mut SqlContext, f: &Found) -> Result<Value> {
    let Some(text) = &f.def.default else {
        return Ok(Value::Null);
    };
    let e = crate::parse::parse_expr(text, ctx.naming)?;
    let v = Evaluator { ctx, params: &[] }.eval(txn, &e, &Env::default())?;
    super::cast::cast(&v, &f.data_type, ctx.decfloat_rounding)
}

/// A reference `table.name` or `name` that is no column: the variable's
/// value, `None` when no variable has the name.
pub fn reference(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    table: Option<&str>,
    name: &str,
    delimited: bool,
) -> Result<Option<Value>> {
    let on = ObjectName {
        schema: table.map(|t| Ident {
            value: t.to_string(),
            delimited: false,
        }),
        name: Ident {
            value: name.to_string(),
            delimited,
        },
    };
    match find(txn, ctx, &on)? {
        Some(f) => value(txn, ctx, &f).map(Some),
        None => Ok(None),
    }
}

/// `CREATE [OR REPLACE] VARIABLE`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, c: &CreateVariable) -> Result<()> {
    super::cast::check_column(&c.data_type)?;
    super::qtemp::refuse_in_qtemp(&c.name, "global variable")?;
    let lib = ctx.library_for(txn, &c.name)?;
    if let Some(old) = find(txn, ctx, &c.name)? {
        if !c.replace {
            return Err(err(
                -601,
                "42710",
                format!("variable {} exists", c.name.name.value),
            ));
        }
        remove(txn, ctx, lib, &c.name, old.id)?;
    }
    let long = c.name.name.value.len() > 10;
    let sys = if long {
        super::names::generate(txn, lib, types::SRVPGM, &c.name.name.value, &|_| false)?
    } else {
        SqlContext::sys_name(&c.name.name)?
    };
    let now = ctx.clock();
    let id = ops::create(
        txn,
        &store::obj::CreateParams {
            library: lib,
            name: sys,
            type_code: types::SRVPGM,
            attribute: String::new(),
            text: String::new(),
            owner: ctx.user.clone(),
            system: "QS400".into(),
            now,
            by_system: false,
        },
    )?;
    let def = Variable {
        data_type: crate::pl::types::type_text(&c.data_type),
        default: c.default.as_ref().map(|(_, t)| t.clone()),
        constant: c.constant,
    };
    let mut info = SqlInfo::new(Kind::Variable);
    if long {
        info.set(tag::NAME, c.name.name.value.as_bytes().to_vec());
    }
    info.set(tag::VARIABLE, def.encode());
    sqlinf::save(txn, id, &info, now)?;
    names::put(
        txn,
        lib,
        Space::Variable,
        &c.name.name.value,
        Target { id, system: sys },
    )?;
    Ok(())
}

fn remove(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    lib: ObjectId,
    name: &ObjectName,
    id: ObjectId,
) -> Result<()> {
    db::auth::check(txn, "obj.delete", id).map_err(SqlError::from)?;
    names::remove(txn, lib, Space::Variable, &name.name.value)?;
    ops::delete(txn, ctx.job, id)?;
    ctx.vars.set.remove(&id);
    Ok(())
}

/// `DROP VARIABLE`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<()> {
    let f = find(txn, ctx, name)?.ok_or_else(|| SqlError::ObjectNotFound {
        kind: "variable",
        name: name.name.value.clone(),
    })?;
    let lib = ctx.library_for(txn, name)?;
    remove(txn, ctx, lib, name, f.id)
}

/// `SET v = e, ...`: every value is computed before any is assigned. A
/// string that is no number gives a numeric variable the null value and
/// warning `SQL0420`, as the primary does.
pub fn set(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    targets: &[ObjectName],
    values: &[Option<Expr>],
) -> Result<()> {
    let mut found = Vec::with_capacity(targets.len());
    for t in targets {
        let f = find(txn, ctx, t)?.ok_or_else(|| not_found(t))?;
        if f.def.constant {
            return Err(err(
                -151,
                "42808",
                format!("variable {} is a constant", t.name.value),
            ));
        }
        found.push(f);
    }
    let mut out = Vec::with_capacity(found.len());
    for (f, e) in found.iter().zip(values) {
        let v = match e {
            None => default_of(txn, ctx, f)?,
            Some(e) => {
                let v = Evaluator { ctx, params }.eval(txn, e, &Env::default())?;
                match super::cast::cast(&v, &f.data_type, ctx.decfloat_rounding) {
                    Err(e) if e.sqlcode() == -420 => {
                        ctx.mapping_warning = Some((420, "01565"));
                        Value::Null
                    }
                    r => r?,
                }
            }
        };
        out.push(v);
    }
    for (f, v) in found.iter().zip(out) {
        ctx.vars.set.insert(f.id, v);
    }
    Ok(())
}

/// A variable listed: system name, object, SQL name, definition,
/// created, owner.
pub type Listed = (Name, ObjectId, String, Variable, u64, String);

/// The variables of a library, with their SQL names.
pub fn in_library(txn: &mut Txn<'_>, lib: ObjectId) -> Result<Vec<Listed>> {
    let mut out = Vec::new();
    for (sys, id) in ops::list(txn, lib, Some(types::SRVPGM))? {
        let Ok(Some(info)) = sqlinf::load(txn, id) else {
            continue;
        };
        if info.kind != Kind::Variable {
            continue;
        }
        let Some(Ok(def)) = info.first(tag::VARIABLE).map(Variable::decode) else {
            continue;
        };
        let (_, h) = ops::read_header(txn, id)?;
        let name = info.name().unwrap_or_else(|| sys.name.to_string());
        out.push((
            sys.name,
            id,
            name,
            def,
            h.created,
            ccsid::decode_trimmed(&h.owner),
        ));
    }
    Ok(out)
}
