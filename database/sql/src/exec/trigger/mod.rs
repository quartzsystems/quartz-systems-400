// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE TRIGGER`, `DROP TRIGGER`, `LABEL ON` and `COMMENT ON TRIGGER`.
//! An SQL trigger is a trigger of its table as `ADDPFTRG` adds one (a
//! `TRIGGER` entry, `db::trg`), flagged SQL and carrying its definition,
//! over a program compiled from its body in the trigger's schema
//! ([`program`]); the name is in the schema's index (triggers' space),
//! standing for the table, with the program's system name. Record I/O
//! and SQL changes call it alike. As on the primary
//! (`tools/conformance/sql/oracle/trigger/primary-facts.md`): the program
//! is named as an SQL procedure's, a `BEFORE` trigger is `DB2ROW`
//! (`SQL7051` when the mode was not given), an `AFTER` one `DB2SQL`
//! unless told, and a failing body is `SQL0723`.

pub mod program;
mod refs;

use db::sqlinf::names::{self, Space, Target};
use db::trg::ops::{NewTrg, Pick};
use db::trg::{Event, MtAction, SqlDef, Threadsafe, Time, Trigger};
use qsname::Name;
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::routine::create::{Cat, body_definition, commit_option, make_program, program_name};
use super::{SqlContext, TableInfo, names as sqlnames};
use crate::ast::{CreateTrigger, Ident, ObjectName, PlKind, PlStmt, TriggerEvent, TriggerTime};
use crate::error::{Result, SqlError};

fn unsupported(what: &str) -> SqlError {
    SqlError::Unsupported { what: what.into() }
}

fn coded(code: i32, state: &'static str, what: String) -> SqlError {
    SqlError::Coded { code, state, what }
}

/// `SQL0696`: a correlation name the trigger cannot have.
fn bad_correlation(name: &Ident, reason: u8) -> SqlError {
    coded(-696, "42898", format!("correlation name or table {} is not valid here (reason {reason})", name.value))
}

/// The trigger named `name`: its schema, and what its name stands for.
fn find(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName) -> Result<(ObjectId, Option<Target>)> {
    let lib = ctx.library_for(txn, name)?;
    Ok((lib, names::find(txn, lib, Space::Trigger, &name.name.value)?))
}

/// The SQL trigger `name` in schema `lib` on file `file`.
fn trigger_of(txn: &mut Txn<'_>, lib: ObjectId, file: ObjectId, name: &str) -> Result<Option<Trigger>> {
    let lib_name = ops::read_header(txn, lib)?.1.name;
    Ok(db::trg::list(txn, file)?.into_iter().find(|t| t.name == name && t.library == lib_name))
}

/// Delete program `pgm` of `lib`, when it is there.
fn delete_program(txn: &mut Txn<'_>, ctx: &SqlContext, lib: Name, pgm: Name) -> Result<()> {
    if let Some(id) = program_id(txn, ctx, lib, pgm)? {
        ops::delete(txn, ctx.job, id)?;
    }
    Ok(())
}

/// A trigger's program, when it is there.
fn program_id(txn: &mut Txn<'_>, ctx: &SqlContext, lib: Name, pgm: Name) -> Result<Option<ObjectId>> {
    let Ok(l) = ops::find_library(txn, ctx.qsys, lib) else { return Ok(None) };
    Ok(Library::load(txn, l.id)?.lookup(txn, types::PGM, pgm)?)
}

/// `DROP TRIGGER` and `CREATE OR REPLACE TRIGGER` delete the trigger's
/// program: `*OBJEXIST` to it (`obj.delete`), as for a routine's.
fn check_program_delete(txn: &mut Txn<'_>, ctx: &SqlContext, t: &Trigger) -> Result<()> {
    if let Some(id) = program_id(txn, ctx, t.program.0, t.program.1)? {
        db::auth::check(txn, "obj.delete", id)?;
    }
    Ok(())
}

/// The mode, and the warning `SQL7051` for a `BEFORE` trigger whose
/// mode was not given (it is `DB2ROW`).
fn mode(ctx: &mut SqlContext, t: &CreateTrigger) -> Result<bool> {
    if t.statement {
        if t.time == TriggerTime::Before || t.db2sql == Some(false) {
            return Err(coded(-628, "42613", "FOR EACH STATEMENT with BEFORE or MODE DB2ROW".into()));
        }
        // BLOCKED(rel): a statement trigger is called once per statement
        // with its transition tables, which the trigger buffer has no
        // room for.
        return Err(unsupported("FOR EACH STATEMENT triggers"));
    }
    Ok(match (t.time, t.db2sql) {
        (TriggerTime::Before, None | Some(true)) => {
            ctx.warning = Some(crate::sqlca::Warning::coded(
                7051,
                "01647",
                &format!("MODE DB2SQL before trigger {} is MODE DB2ROW", t.name.name.value),
            ));
            false
        }
        (_, Some(m)) => m,
        (_, None) => true,
    })
}

/// The checks of `REFERENCING` against the event and time.
fn check_referencing(t: &CreateTrigger, db2sql: bool) -> Result<()> {
    let event = &t.event;
    if let (TriggerEvent::Insert, Some(o)) = (event, t.old_row.as_ref().or(t.old_table.as_ref())) {
        return Err(bad_correlation(o, 2));
    }
    if let (TriggerEvent::Delete, Some(n)) = (event, t.new_row.as_ref().or(t.new_table.as_ref())) {
        return Err(bad_correlation(n, 1));
    }
    if let Some(tb) = t.old_table.as_ref().or(t.new_table.as_ref()) {
        if t.time == TriggerTime::Before || !db2sql {
            return Err(bad_correlation(tb, 3));
        }
        // BLOCKED(rel): transition tables of a row trigger.
        return Err(unsupported("OLD TABLE and NEW TABLE (transition tables)"));
    }
    Ok(())
}

/// The table a trigger is on: a physical file for `BEFORE` and `AFTER`.
fn subject(txn: &mut Txn<'_>, ctx: &SqlContext, t: &CreateTrigger) -> Result<TableInfo> {
    let id = ctx.resolve_file(txn, &t.table)?;
    let view = super::view::is_view(txn, id);
    let not_type = |code| coded(code, "42809", format!("{} is not of the type the trigger needs", t.table.name.value));
    match t.time {
        TriggerTime::InsteadOf if view => {
            // BLOCKED(rel): an insert, update or delete of a view runs its
            // INSTEAD OF triggers in place of the change.
            Err(unsupported("INSTEAD OF triggers"))
        }
        TriggerTime::InsteadOf => Err(not_type(-159)),
        _ if view => Err(not_type(-156)),
        _ => {
            let info = ctx.resolve_table(txn, &t.table)?;
            if info.desc.attr.kind != db::file::FileKind::Physical {
                return Err(not_type(-156));
            }
            Ok(info)
        }
    }
}

/// `CREATE [OR REPLACE] TRIGGER`.
#[allow(clippy::too_many_lines)] // the clauses in order
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTrigger) -> Result<()> {
    sqlnames::check_length(&t.name.name)?;
    let lib = ctx.library_for(txn, &t.name)?;
    let lib_name = ops::read_header(txn, lib)?.1.name;
    let schema = sqlnames::schema_name(txn, lib);
    let db2sql = mode(ctx, t)?;
    check_referencing(t, db2sql)?;
    let info = subject(txn, ctx, t)?;
    super::qtemp::refuse_declared(ctx, info.id, "a trigger")?;
    super::qtemp::unsupported_in_qtemp(&t.name, "a trigger")?;
    db::auth::check(txn, "file.change", info.id)?;
    let before = t.time == TriggerTime::Before;
    refs::check(&t.body, before)?;
    let options = t.options.clone();
    commit_option(&options)?;
    let name = t.name.name.value.clone();
    let (_, existing) = find(txn, ctx, &t.name)?;
    let old = match existing {
        Some(_) if !t.replace => {
            return Err(SqlError::Exists { name: format!("{name} in {schema}"), kind: "trigger" });
        }
        Some(e) => trigger_of(txn, lib, e.id, &name)?.map(|o| (e.id, o)),
        None => None,
    };
    if let Some((_, o)) = &old {
        check_program_delete(txn, ctx, o)?;
    }
    let event = match &t.event {
        TriggerEvent::Insert => Event::Insert,
        TriggerEvent::Delete => Event::Delete,
        TriggerEvent::Update(_) => Event::Update,
    };
    let mut update_cols = Vec::new();
    if let TriggerEvent::Update(cols) = &t.event {
        for c in cols {
            let (_, f) = info.field(&c.value).ok_or_else(|| SqlError::ColumnNotFound { name: c.value.clone() })?;
            update_cols.push(f.name);
        }
    }
    // The transition variables.
    let cols = program::columns(&info.format);
    let corr = |i: &Option<Ident>| i.as_ref().map(|c| c.value.clone());
    let (old_row, new_row) = (corr(&t.old_row), corr(&t.new_row));
    let both: Vec<&str> = [old_row.as_deref(), new_row.as_deref()].into_iter().flatten().collect();
    let cond_text = t.when.as_ref().map(|w| w.text.clone()).unwrap_or_default();
    let used: Vec<String> = refs::qualified_uses(&format!("{cond_text} {}", t.body_text), &both);
    for u in &used {
        let Some(c) = cols.iter().find(|c| c.names.iter().any(|n| n == u)) else { continue };
        if c.ty.is_none() {
            // BLOCKED(rel): a column whose storage is not a routine variable's.
            return Err(unsupported(&format!("transition variable {u} of type {} in a trigger", c.sql_type)));
        }
    }
    let program = program_name(txn, lib, &t.name.name)?;
    let pgm = program.to_string();
    let (mut sig, slots) = program::signature(&pgm, &name, &cols, old_row.as_deref(), new_row.as_deref(), before);
    sig.naming = ctx.naming;
    sig.commit = commit_option(&options)?;
    let mut body = (*t.body).clone();
    if before {
        refs::not_atomic(&mut body);
    }
    // `WHEN`: the body runs when the condition is true.
    if let Some(w) = &t.when {
        body = PlStmt { label: None, line: t.body.line, kind: PlKind::If { branches: vec![(w.clone(), vec![body])], otherwise: None } };
    }
    let mut cat = Cat { txn: &mut *txn, ctx: &mut *ctx, deps: Vec::new() };
    let mut qir = crate::pl::compile(&sig, &body, &mut cat)?;
    let parts = db::trg::buffer::Parts::of(info.format.format.record_len as usize, info.format.format.fields.len());
    qir.push_str(&program::entry(&pgm, (&name, &schema), &cols, &sig, &slots, parts, before));
    let made = make_program(txn, ctx, lib, program, &qir, "")?;
    // The trigger is added as ADDPFTRG adds one: *EXECUTE to its program.
    db::auth::check(txn, "pgm.trigger", made)?;
    // What the catalogs show.
    let mut def = SqlDef {
        old_row: old_row.clone().unwrap_or_default(),
        new_row: new_row.clone().unwrap_or_default(),
        condition: if t.when.is_some() { body_definition(&format!("( {cond_text} )")) } else { String::new() },
        ..SqlDef::default()
    };
    def.statement = refs::statement_text(&t.body_text, &mut |schema, table| {
        let on = ObjectName { schema: schema.map(|s| Ident { value: s.to_string(), delimited: false }), name: Ident { value: table.to_string(), delimited: false } };
        let Ok(id) = ctx.resolve_file(txn, &on) else { return Vec::new() };
        let Ok(d) = db::file::describe(txn, id) else { return Vec::new() };
        d.formats.first().map(|f| f.format.fields.iter().map(|x| x.alias.clone().unwrap_or_else(|| x.name.to_string())).collect()).unwrap_or_default()
    });
    let file_name = info.desc.name();
    let file_lib = info.desc.library_name;
    // Each column once in the condition and once in the statement, as
    // the primary lists them.
    for part in [cond_text.as_str(), t.body_text.as_str()] {
        let mut seen: Vec<Name> = Vec::new();
        for u in refs::qualified_uses(part, &both) {
            let Some(c) = cols.iter().find(|c| c.names.contains(&u)) else { continue };
            let n = Name::parse(&c.names[0]).unwrap_or(file_name);
            if !seen.contains(&n) {
                seen.push(n);
                def.columns.push((file_lib, file_name, n));
            }
        }
    }
    for n in refs::tables(&t.body_text) {
        let on = ObjectName { schema: n.schema.map(|s| Ident { value: s, delimited: false }), name: Ident { value: n.name, delimited: false } };
        let Ok(id) = ctx.resolve_file(txn, &on) else { continue };
        let (_, h) = ops::read_header(txn, id)?;
        let (_, lh) = ops::read_header(txn, h.library)?;
        if !def.objects.contains(&(lh.name, h.name)) {
            def.objects.push((lh.name, h.name));
        }
        let Some(listed) = n.insert else { continue };
        let Ok(d) = db::file::describe(txn, id) else { continue };
        let Some(fmt) = d.formats.first() else { continue };
        let mut fields: Vec<Name> = fmt
            .format
            .fields
            .iter()
            .filter(|f| listed.is_empty() || listed.iter().any(|c| *c == f.name.to_string() || f.alias.as_deref() == Some(c.as_str())))
            .map(|f| f.name)
            .collect();
        fields.sort_by_key(ToString::to_string);
        def.columns.extend(fields.into_iter().map(|f| (lh.name, h.name, f)));
    }
    // The old trigger goes; its program after the new one is made, so the
    // new one has a name of its own, as on the primary.
    if let Some((file, o)) = &old {
        if *file != info.id {
            db::trg::ops::remove(txn, *file, &Pick::Named(name.clone(), Some(lib_name)), ctx.clock())?;
        }
        delete_program(txn, ctx, o.program.0, o.program.1)?;
    }
    let new = NewTrg {
        name: Some(name.clone()),
        library: Some(lib_name),
        alwrepchg: true,
        replace: old.as_ref().is_some_and(|(f, _)| *f == info.id),
        threadsafe: Threadsafe::Yes,
        mt_action: MtAction::Run,
        definer: ctx.user.clone(),
        ..NewTrg::new(time_of(t.time), event, (lib_name, program))
    };
    db::trg::ops::add(txn, info.id, &new, ctx.clock())?;
    let mut all = db::trg::list(txn, info.id)?;
    if let Some(e) = all.iter_mut().find(|x| x.name == name && x.library == lib_name) {
        e.sql = true;
        e.db2sql = db2sql;
        e.statement = t.statement;
        e.columns = update_cols;
        e.def = Some(Box::new(def));
    }
    db::trg::store(txn, info.id, &all, ctx.clock())?;
    names::put(txn, lib, Space::Trigger, &name, Target { id: info.id, system: program })?;
    Ok(())
}

fn time_of(t: TriggerTime) -> Time {
    match t {
        TriggerTime::Before => Time::Before,
        TriggerTime::After => Time::After,
        TriggerTime::InsteadOf => Time::InsteadOf,
    }
}

/// `DROP TRIGGER`: the trigger and its program.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName) -> Result<()> {
    let not_found = || SqlError::ObjectNotFound { name: name.to_string(), kind: "trigger" };
    let (lib, found) = find(txn, ctx, name)?;
    let Some(target) = found else { return Err(not_found()) };
    let lib_name = ops::read_header(txn, lib)?.1.name;
    if let Some(t) = trigger_of(txn, lib, target.id, &name.name.value)? {
        db::auth::check(txn, "file.change", target.id)?;
        check_program_delete(txn, ctx, &t)?;
        db::trg::ops::remove(txn, target.id, &Pick::Named(t.name.clone(), Some(lib_name)), ctx.clock())?;
        delete_program(txn, ctx, t.program.0, t.program.1)?;
    }
    names::remove(txn, lib, Space::Trigger, &name.name.value)?;
    Ok(())
}

/// A table goes: its SQL triggers' programs and names go with it.
pub fn drop_all(txn: &mut Txn<'_>, ctx: &SqlContext, file: ObjectId) -> Result<()> {
    for t in db::trg::list(txn, file)?.into_iter().filter(|t| t.sql) {
        delete_program(txn, ctx, t.program.0, t.program.1)?;
        if let Ok(l) = ops::find_library(txn, ctx.qsys, t.library)
            && names::find(txn, l.id, Space::Trigger, &t.name)?.is_some_and(|x| x.id == file)
        {
            names::remove(txn, l.id, Space::Trigger, &t.name)?;
        }
    }
    Ok(())
}

/// `LABEL ON TRIGGER` (its text) and `COMMENT ON TRIGGER` (its long
/// comment).
pub fn label(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &ObjectName, l: &crate::ast::Label) -> Result<()> {
    let (lib, found) = find(txn, ctx, name)?;
    let target = found.ok_or_else(|| SqlError::ObjectNotFound { name: name.to_string(), kind: "trigger" })?;
    // The ALTER privilege on the subject table, as DROP TRIGGER needs.
    db::auth::check(txn, "file.change", target.id)?;
    let lib_name = ops::read_header(txn, lib)?.1.name;
    let text = l.text.clone().unwrap_or_default();
    let mut all = db::trg::list(txn, target.id)?;
    let Some(t) = all.iter_mut().find(|t| t.name == name.name.value && t.library == lib_name) else {
        return Err(SqlError::ObjectNotFound { name: name.to_string(), kind: "trigger" });
    };
    if l.is_text {
        t.def.get_or_insert_with(Box::default).comment = text.chars().take(2000).collect();
    } else {
        t.text = text.chars().take(50).collect();
    }
    db::trg::store(txn, target.id, &all, ctx.clock())?;
    Ok(())
}
