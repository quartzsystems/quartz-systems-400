// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QTEMP` for SQL: `DECLARE GLOBAL TEMPORARY TABLE` and the tables,
//! views, indexes and aliases a job makes in `QTEMP`.
//!
//! A job has one `QTEMP`, which CL, RPG, record I/O and SQL all reach:
//! a root in ASP 1 in no other library ([`ops::create_temporary_root`]),
//! so no listing, catalog view or save sees it. The storage service
//! makes it when the job starts and hands it to the job's SQL contexts
//! ([`Qtemp::adopt`]); a context without a job makes its own at first
//! use. The job's end drops it ([`drop_library`]); the next start of the
//! service drops what a stopped system left ([`purge`]). `SESSION.name`
//! finds only what was not made by a plain `CREATE TABLE QTEMP.name`
//! (the primary's rule).

use std::cell::RefCell;
use std::rc::Rc;

use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::{SqlContext, names};
use crate::ast::{DeclareTemp, Ident, ObjectName};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The library name.
pub const QTEMP: &str = "QTEMP";
/// The qualifier of a declared temporary table.
pub const SESSION: &str = "SESSION";

/// A declared temporary table's options.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct Declared {
    file: ObjectId,
    delete_rows: bool,
    not_logged: bool,
    rollback_preserve: bool,
    /// Changed in the unit of work now.
    changed: bool,
}

#[derive(Debug, Default)]
struct State {
    /// The library and when it was made (an aborted transaction's
    /// object ID may be another's later).
    lib: Option<(ObjectId, u64)>,
    declared: Vec<Declared>,
    /// Tables of `CREATE TABLE QTEMP.x`, which `SESSION` does not find.
    plain: Vec<ObjectId>,
}

/// A job's SQL `QTEMP`, shared by its SQL contexts.
#[derive(Clone, Debug, Default)]
pub struct Qtemp(Rc<RefCell<State>>);

impl Qtemp {
    /// The library, when it was made and is still there.
    pub fn library(&self, txn: &mut Txn<'_>) -> Option<ObjectId> {
        let (id, made) = self.0.borrow().lib?;
        match ops::read_header(txn, id) {
            Ok((_, h)) if h.temporary && h.library == id && h.created == made => Some(id),
            _ => {
                *self.0.borrow_mut() = State::default();
                None
            }
        }
    }

    /// The job's `QTEMP` is `lib`, made at `created` when the job started
    /// (the storage service's one `QTEMP` per job).
    pub fn adopt(&self, lib: ObjectId, created: u64) {
        *self.0.borrow_mut() = State { lib: Some((lib, created)), ..State::default() };
    }

    /// The library made now, if a job's end left one to drop.
    #[must_use]
    pub fn take(&self) -> Option<ObjectId> {
        let s = std::mem::take(&mut *self.0.borrow_mut());
        s.lib.map(|(id, _)| id)
    }

    fn declared(&self, id: ObjectId) -> Option<Declared> {
        self.0.borrow().declared.iter().find(|d| d.file == id).copied()
    }

    fn forget(&self, id: ObjectId) {
        let mut s = self.0.borrow_mut();
        s.declared.retain(|d| d.file != id);
        s.plain.retain(|p| *p != id);
    }

    /// Whether `file` is a declared temporary table.
    #[must_use]
    pub fn is_declared(&self, file: ObjectId) -> bool {
        self.declared(file).is_some()
    }

    /// Whether changes to `file` are outside commitment control (`NOT
    /// LOGGED`).
    #[must_use]
    pub fn not_logged(&self, file: ObjectId) -> bool {
        self.declared(file).is_some_and(|d| d.not_logged)
    }

    /// `file` is opened for a change.
    pub fn changing(&self, file: ObjectId) {
        if let Some(d) = self.0.borrow_mut().declared.iter_mut().find(|d| d.file == file) {
            d.changed = true;
        }
    }

    /// `CREATE TABLE QTEMP.x` made `file`.
    pub fn plain_table(&self, file: ObjectId) {
        self.0.borrow_mut().plain.push(file);
    }

    fn found_by_session(&self, id: ObjectId) -> bool {
        !self.0.borrow().plain.contains(&id)
    }
}

/// Whether `n` is `QTEMP`.
#[must_use]
pub fn is_qtemp(n: Name) -> bool {
    n.to_string() == QTEMP
}

/// How a qualifier reaches `QTEMP`: `Some(true)` for `SESSION`,
/// `Some(false)` for `QTEMP`, `None` otherwise.
#[must_use]
pub fn qualifier(name: &ObjectName) -> Option<bool> {
    let s = name.schema.as_ref()?;
    match s.value.as_str() {
        SESSION => Some(true),
        QTEMP => Some(false),
        _ => None,
    }
}

impl SqlContext {
    /// The job's SQL `QTEMP`, when it has one.
    pub fn qtemp_library(&self, txn: &mut Txn<'_>) -> Option<ObjectId> {
        self.qtemp.library(txn)
    }

    /// The job's SQL `QTEMP`, made now when it has none.
    pub fn make_qtemp(&self, txn: &mut Txn<'_>) -> Result<ObjectId> {
        if let Some(id) = self.qtemp.library(txn) {
            return Ok(id);
        }
        let now = self.clock();
        let owner = if self.user.is_empty() { "QSYS" } else { self.user.as_str() };
        let clean = txn.is_clean();
        let id = ops::create_temporary_root(txn, Name::parse(QTEMP).expect("valid"), owner, now)?;
        // Made durable at once when nothing else is pending, so no
        // rollback of the statement takes it back.
        if clean {
            txn.commit_part()?;
        }
        let made = ops::read_header(txn, id)?.1.created;
        self.qtemp.0.borrow_mut().lib = Some((id, made));
        Ok(id)
    }

    /// A file named through `QTEMP` or `SESSION`.
    pub(crate) fn resolve_in_qtemp(&self, txn: &mut Txn<'_>, name: &ObjectName, session: bool) -> Result<Option<ObjectId>> {
        let Some(lib) = self.qtemp_library(txn) else { return Ok(None) };
        let id = if let Some(n) = names::direct(&name.name) {
            store::obj::library::Library::load(txn, lib)?.lookup(txn, types::FILE, n)?
        } else {
            let mut found = None;
            for space in [db::sqlinf::names::Space::Table, db::sqlinf::names::Space::Index] {
                if let Some(id) = names::find(txn, lib, space, &name.name.value)? {
                    found = Some(id);
                    break;
                }
            }
            found
        };
        Ok(id.filter(|&id| !session || self.qtemp.found_by_session(id)))
    }
}

/// `SQL0526`: what a declared temporary table cannot have or be used for.
fn not_for_declared(what: &str) -> SqlError {
    SqlError::Coded { code: -526, state: "42995", what: format!("{what} does not apply to a declared temporary table") }
}

/// Refuse `what` on `file` when it is a declared temporary table.
pub fn refuse_declared(ctx: &SqlContext, file: ObjectId, what: &str) -> Result<()> {
    if ctx.qtemp.is_declared(file) { Err(not_for_declared(what)) } else { Ok(()) }
}

/// `SQL0707`: an object kind `QTEMP` cannot hold.
pub fn refuse_in_qtemp(name: &ObjectName, kind: &str) -> Result<()> {
    if qualifier(name) == Some(false) {
        return Err(SqlError::Coded { code: -707, state: "42939", what: format!("a {kind} cannot be made in QTEMP") });
    }
    Ok(())
}

/// `DECLARE GLOBAL TEMPORARY TABLE`: rows inserted (`AS ... WITH DATA`).
pub fn declare(txn: &mut Txn<'_>, ctx: &mut SqlContext, d: &DeclareTemp, params: &[Value]) -> Result<u64> {
    let t = &d.table;
    if let Some(s) = &t.name.schema
        && s.value != SESSION
    {
        return Err(SqlError::SchemaNotValid { schema: s.value.clone(), name: t.name.name.value.clone() });
    }
    if !t.constraints.is_empty() || t.columns.iter().any(|c| !c.constraints.is_empty()) {
        return Err(not_for_declared("a constraint"));
    }
    let mut table = t.clone();
    table.name = ObjectName { schema: Some(Ident { value: QTEMP.into(), delimited: false }), name: t.name.name.clone() };
    let lib = ctx.make_qtemp(txn)?;
    if let Some(old) = ctx.resolve_in_qtemp(txn, &table.name, false)? {
        if !d.replace {
            return Err(SqlError::Exists { name: t.name.name.value.clone(), kind: "*FILE" });
        }
        drop_file(txn, ctx, old)?;
    }
    let count = super::table::create(txn, ctx, &table, params)?;
    let id = ctx.resolve_in_qtemp(txn, &table.name, false)?.ok_or_else(|| SqlError::ObjectNotFound { name: t.name.name.value.clone(), kind: "table" })?;
    ctx.qtemp.0.borrow_mut().plain.retain(|p| *p != id);
    if !d.not_logged {
        journal(txn, ctx, lib, id)?;
    }
    ctx.qtemp.0.borrow_mut().declared.push(Declared {
        file: id,
        delete_rows: d.delete_rows,
        not_logged: d.not_logged,
        rollback_preserve: d.rollback_preserve,
        changed: false,
    });
    // As 7.5 reports it: logged, but in no schema's journal.
    ctx.warning = (!d.not_logged).then(|| crate::sqlca::Warning::not_journaled(&t.name.name.value, QTEMP));
    Ok(count)
}

/// A logged declared temporary table is journaled to `QTEMP/QSQJRN`, so
/// a rollback undoes its changes.
fn journal(txn: &mut Txn<'_>, ctx: &SqlContext, lib: ObjectId, file: ObjectId) -> Result<()> {
    let fixed = |n: &str| Name::parse(n).expect("a valid name");
    let now = ctx.clock();
    let l = store::obj::library::Library::load(txn, lib)?;
    let journal = if let Some(j) = l.lookup(txn, jrn::JRN, fixed(super::schema::JOURNAL))? {
        j
    } else {
            let rcv = jrn::rcv::crtjrnrcv(txn, lib, fixed(super::schema::RECEIVER), &jrn::rcv::CrtJrnRcvOpts { now, ..jrn::rcv::CrtJrnRcvOpts::default() })
                .map_err(db::DbError::from)?;
            jrn::journal::crtjrn(txn, lib, fixed(super::schema::JOURNAL), rcv, &jrn::journal::CrtJrnOpts { now, ..jrn::journal::CrtJrnOpts::default() })
                .map_err(db::DbError::from)?
    };
    db::journal::strjrnpf(txn, ctx.job, file, journal, true, false, now)?;
    Ok(())
}

/// Drop a file of `QTEMP` and what depends on it.
fn drop_file(txn: &mut Txn<'_>, ctx: &mut SqlContext, id: ObjectId) -> Result<()> {
    for d in super::drop::dependents(txn, ctx, id)? {
        ctx.qtemp.forget(d);
    }
    ctx.qtemp.forget(id);
    super::drop::drop_with_dependents(txn, ctx, id)
}

/// A file was dropped: what the job kept of it goes.
pub fn dropped(ctx: &SqlContext, id: ObjectId) {
    ctx.qtemp.forget(id);
}

/// The unit of work ended: `ON COMMIT DELETE ROWS` tables are cleared
/// unless a `WITH HOLD` cursor is open on them; after a rollback, a
/// `NOT LOGGED` table the unit changed is cleared unless `ON ROLLBACK
/// PRESERVE ROWS`.
pub fn end_of_unit(txn: &mut Txn<'_>, ctx: &mut SqlContext, rollback: bool) -> Result<()> {
    let declared: Vec<Declared> = ctx.qtemp.0.borrow().declared.clone();
    for d in declared {
        let held = ctx.cursors.values().any(|c| c.open && c.hold && c.table.as_ref().is_some_and(|t| t.id == d.file));
        let clear = if rollback && d.not_logged { d.changed && !d.rollback_preserve } else { d.delete_rows && !held };
        if clear && ops::read_header(txn, d.file).is_ok() {
            db::util::clrpfm(txn, ctx.job, d.file, MemberSpec::First)?;
        }
    }
    for d in &mut ctx.qtemp.0.borrow_mut().declared {
        d.changed = false;
    }
    Ok(())
}

/// Drop a job's SQL `QTEMP` and everything in it.
pub fn drop_library(txn: &mut Txn<'_>, qsys: ObjectId, job: JobHandle, lib: ObjectId) -> Result<()> {
    let mut ctx = SqlContext::new(qsys, job, ops::NameContext::default());
    ctx.qtemp.0.borrow_mut().lib = Some((lib, ops::read_header(txn, lib)?.1.created));
    let objects: Vec<ObjectId> = ops::list(txn, lib, None)?.into_iter().map(|(_, id)| id).filter(|&id| id != lib).collect();
    for &id in objects.iter().filter(|id| id.type_code == types::FILE) {
        if ops::read_header(txn, id).is_err() {
            continue;
        }
        // A display, printer or DDM file is no database file.
        if db::file::describe(txn, id).is_ok() {
            super::drop::drop_with_dependents(txn, &mut ctx, id)?;
        } else {
            ops::delete(txn, job, id)?;
        }
    }
    let now = ctx.clock();
    for &id in objects.iter().filter(|id| id.type_code == jrn::JRN) {
        jrn::journal::dltjrn(txn, job, id, now).map_err(db::DbError::from)?;
    }
    for &id in objects.iter().filter(|id| id.type_code == jrn::JRNRCV) {
        jrn::rcv::dltjrnrcv(txn, job, id).map_err(db::DbError::from)?;
    }
    for &id in objects.iter().filter(|id| ![types::FILE, jrn::JRN, jrn::JRNRCV].contains(&id.type_code)) {
        if ops::read_header(txn, id).is_ok() {
            ops::delete(txn, job, id)?;
        }
    }
    ops::delete(txn, job, lib)?;
    Ok(())
}

/// Drop every job's SQL `QTEMP` a stopped system left; how many.
pub fn purge(txn: &mut Txn<'_>, qsys: ObjectId, job: JobHandle) -> Result<usize> {
    let roots = ops::temporary_roots(txn)?;
    for &lib in &roots {
        drop_library(txn, qsys, job, lib)?;
    }
    Ok(roots.len())
}

/// The library a stored name means: `QTEMP` the job's.
pub fn find_library(txn: &mut Txn<'_>, ctx: &SqlContext, n: Name) -> Option<ObjectId> {
    if is_qtemp(n) {
        return ctx.qtemp_library(txn);
    }
    ops::find_library(txn, ctx.qsys, n).ok().map(|f| f.id)
}

/// `SQL0084`: `QTEMP` holds no SQL objects of this kind yet.
pub fn unsupported_in_qtemp(name: &ObjectName, what: &str) -> Result<()> {
    if qualifier(name).is_some() {
        // BLOCKED(sql): routines, types and triggers in QTEMP need their
        // programs and catalog entries kept per job.
        return Err(SqlError::Unsupported { what: format!("{what} in QTEMP") });
    }
    Ok(())
}
