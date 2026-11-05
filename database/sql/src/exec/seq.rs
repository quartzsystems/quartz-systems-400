// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sequences: `CREATE`, `ALTER` and `DROP SEQUENCE`, `NEXT VALUE FOR`,
//! `PREVIOUS VALUE FOR`, and the job's last values (with the identity
//! column's, `IDENTITY_VAL_LOCAL`). A sequence is a `*DTAARA`, as IBM
//! keeps one, whose `*SQLINF` holds its attributes and next value
//! (`SEQUENCE` entry): the value moves in the statement's transaction,
//! so it survives an IPL and is never given twice.

use std::collections::HashMap;

use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::seq::Sequence;
use db::sqlinf::{self, Kind, SqlInfo, tag};
use qdecimal::Decimal;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{CreateSequence, DataType, Identity, ObjectName};
use crate::error::{Result, SqlError};
use crate::value::Value;

fn err(sqlcode: i32, sqlstate: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Integrity { sqlcode, sqlstate, what: what.into() }
}

/// What a job remembers of the values it was given.
#[derive(Clone, Debug, Default)]
pub struct JobValues {
    /// The last `NEXT VALUE` of each sequence, for `PREVIOUS VALUE`.
    pub previous: HashMap<ObjectId, Value>,
    /// The last identity value an `INSERT` of one row gave, for
    /// `IDENTITY_VAL_LOCAL()`.
    pub identity: Option<Value>,
}

/// The type code and precision a sequence keeps.
fn type_code(t: &DataType) -> Option<(u8, u8)> {
    Some(match t {
        DataType::SmallInt => (1, 0),
        DataType::Integer => (2, 0),
        DataType::BigInt => (3, 0),
        DataType::Decimal(p, 0) => (4, *p),
        DataType::Numeric(p, 0) => (5, *p),
        _ => return None,
    })
}

fn sql_type(code: u8, p: u8) -> DataType {
    match code {
        1 => DataType::SmallInt,
        3 => DataType::BigInt,
        4 => DataType::Decimal(p, 0),
        5 => DataType::Numeric(p, 0),
        _ => DataType::Integer,
    }
}

/// The type of the values the sequence `name` gives; `None` when there
/// is none of that name.
pub fn data_type(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Option<DataType> {
    resolve(txn, ctx, name).ok().map(|(_, s)| sql_type(s.data_type, s.precision))
}

/// The sequence `name` names, with its state.
pub fn resolve(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<(ObjectId, Sequence)> {
    let lib = ctx.library_for(txn, name)?;
    let not_found = || SqlError::ObjectNotFound { kind: "sequence", name: name.name.value.clone() };
    let id = if let Some(t) = names::find(txn, lib, Space::Sequence, &name.name.value)? {
        t.id
    } else {
            let sys = SqlContext::sys_name(&name.name).map_err(|_| not_found())?;
            let l = store::obj::library::Library::load(txn, lib)?;
            l.lookup(txn, types::DTAARA, sys)?.ok_or_else(not_found)?
    };
    let info = sqlinf::load(txn, id)?.filter(|i| i.kind == Kind::Sequence).ok_or_else(not_found)?;
    let body = info.first(tag::SEQUENCE).ok_or_else(not_found)?;
    Ok((id, Sequence::decode(body)?))
}

fn save(txn: &mut Txn<'_>, id: ObjectId, s: &Sequence, now: u64) -> Result<()> {
    let mut info = sqlinf::load(txn, id)?.unwrap_or_else(|| SqlInfo::new(Kind::Sequence));
    info.set(tag::SEQUENCE, s.encode());
    sqlinf::save(txn, id, &info, now)?;
    Ok(())
}

/// `CREATE SEQUENCE`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, c: &CreateSequence) -> Result<()> {
    if c.options.restart.is_some() {
        return Err(err(-104, "42601", "RESTART is for ALTER SEQUENCE"));
    }
    let t = c.data_type.as_ref().map_or(DataType::Integer, |t| t.data_type.clone());
    let Some((code, p)) = type_code(&t) else {
        return Err(err(-846, "42815", format!("a sequence cannot be of type {t:?}")));
    };
    let range = super::column::type_range(&t).expect("an integer type");
    let attrs = super::column::attrs(&c.options, range, &format!("sequence {}", c.name.name.value))?;
    let lib = ctx.library_for(txn, &c.name)?;
    let sys = match &c.system_name {
        Some(s) => SqlContext::sys_name(s)?,
        None => SqlContext::sys_name(&c.name.name)?,
    };
    if names::find(txn, lib, Space::Sequence, &c.name.name.value)?.is_some() {
        if !c.replace {
            return Err(err(-601, "42710", format!("sequence {} exists", c.name.name.value)));
        }
        drop(txn, ctx, &c.name)?;
    }
    let now = ctx.clock();
    let params = store::obj::CreateParams {
        library: lib,
        name: sys,
        type_code: types::DTAARA,
        attribute: String::new(),
        text: String::new(),
        owner: ctx.user.clone(),
        system: "QS400".into(),
        now,
        by_system: false,
    };
    let id = store::obj::dtaara::create(
        txn,
        &params,
        // As IBM i makes it: *CHAR 2000, marked QSQLSEQ; the state is in
        // the *SQLINF.
        store::obj::dtaara::DataAreaType::Char { len: 2000 },
        &store::obj::dtaara::DataAreaValue::Char("QSQLSEQ".into()),
    )?;
    let mut info = SqlInfo::new(Kind::Sequence);
    info.set(tag::NAME, c.name.name.value.as_bytes().to_vec());
    sqlinf::save(txn, id, &info, now)?;
    save(txn, id, &Sequence { data_type: code, precision: p, attrs, next: attrs.start, exhausted: false, block_end: None }, now)?;
    names::put(txn, lib, Space::Sequence, &c.name.name.value, Target { id, system: sys })?;
    Ok(())
}

/// `ALTER SEQUENCE`.
pub fn alter(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName, o: &Identity) -> Result<()> {
    let (id, mut s) = resolve(txn, ctx, name)?;
    db::auth::check(txn, "sequence.alter", id).map_err(SqlError::from)?;
    s.attrs = merge(&s.attrs, o, super::column::type_range(&sql_type(s.data_type, s.precision)).expect("integer"), &format!("sequence {}", name.name.value))?;
    // Values IBM had cached are passed over; an exhausted sequence goes on
    // from its last value under the new attributes.
    if let Some(be) = s.block_end.take() {
        match s.attrs.after(be) {
            Some(n) => s.next = n,
            None => s.exhausted = true,
        }
    } else if s.exhausted
        && let Some(n) = s.attrs.after(s.next)
    {
        s.next = n;
        s.exhausted = false;
    }
    if let Some(r) = o.restart {
        s.next = r.unwrap_or(s.attrs.start);
        s.exhausted = false;
        s.block_end = None;
        if !s.attrs.holds(s.next) {
            return Err(err(-846, "42815", format!("RESTART WITH {} is outside the range of sequence {}", s.next, name.name.value)));
        }
    }
    save(txn, id, &s, ctx.clock())
}

/// New attributes: `o`'s options over `a`.
pub fn merge(a: &db::sqlinf::seq::SeqAttrs, o: &Identity, range: (i128, i128), what: &str) -> Result<db::sqlinf::seq::SeqAttrs> {
    let up = o.increment.unwrap_or(a.increment) > 0;
    let full = Identity {
        always: a.always,
        start: Some(o.start.unwrap_or(a.start)),
        increment: Some(o.increment.unwrap_or(a.increment)),
        min: Some(Some(match o.min {
            Some(Some(v)) => v,
            Some(None) => if up { 1 } else { range.0 },
            None => a.min,
        })),
        max: Some(Some(match o.max {
            Some(Some(v)) => v,
            Some(None) => if up { range.1 } else { -1 },
            None => a.max,
        })),
        cycle: Some(o.cycle.unwrap_or(a.cycle)),
        cache: Some(o.cache.unwrap_or(a.cache)),
        order: Some(o.order.unwrap_or(a.order)),
        restart: None,
    };
    let mut probe = full.clone();
    // START WITH is checked against the range only when given.
    if o.start.is_none() {
        probe.start = probe.min.flatten();
    }
    let mut r = super::column::attrs(&probe, range, what)?;
    r.start = full.start.expect("set");
    Ok(r)
}

/// `DROP SEQUENCE`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<()> {
    let (id, _) = resolve(txn, ctx, name)?;
    let lib = ctx.library_for(txn, name)?;
    names::remove(txn, lib, Space::Sequence, &name.name.value)?;
    ops::delete(txn, ctx.job, id)?;
    ctx.seq.previous.remove(&id);
    Ok(())
}

fn value_of(s: &Sequence, v: i128) -> Value {
    match s.data_type {
        4 | 5 => Value::Num(Decimal::new(v, 0).expect("in range")),
        _ => Value::int(i64::try_from(v).unwrap_or(i64::MAX)),
    }
}

/// `NEXT VALUE FOR name`.
pub fn next_value(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<Value> {
    let (id, mut s) = resolve(txn, ctx, name)?;
    if s.exhausted {
        return Err(err(-359, "23522", format!("sequence {} has no more values", name.name.value)));
    }
    let v = s.next;
    // The block of values IBM would cache: kept so an ALTER goes on after it.
    let end = *s.block_end.get_or_insert_with(|| s.attrs.block_end(v));
    if v == end {
        s.block_end = None;
    }
    match s.attrs.after(v) {
        Some(n) => s.next = n,
        None => s.exhausted = true,
    }
    save(txn, id, &s, ctx.clock())?;
    let out = value_of(&s, v);
    ctx.seq.previous.insert(id, out.clone());
    Ok(out)
}

/// `PREVIOUS VALUE FOR name`: this job's last.
pub fn previous_value(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<Value> {
    let (id, _) = resolve(txn, ctx, name)?;
    ctx.seq.previous.get(&id).cloned().ok_or_else(|| {
        err(-845, "51035", format!("NEXT VALUE has not been used for sequence {} in this job", name.name.value))
    })
}
