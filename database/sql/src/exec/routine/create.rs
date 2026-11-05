// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE PROCEDURE`, `CREATE FUNCTION` and `DROP` of either. An SQL
//! procedure's body is compiled to a `*PGM` in the schema (its name if
//! that is a system name, else IBM's five characters and a number); an
//! external routine's attributes go into its program's `*SQLINF`.

use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::routine::{ParmDef, Routine, RoutineDef, flag};
use db::sqlinf::{self, Kind, SqlInfo, tag};
use ile::store::{Ctx as IleCtx, ObjSpec};
use ile::{ActGrpAttr, BindOptions, CreateData, ModuleObject, PgmKind};
use qsname::{LibSpec, Name};
use store::obj::library::Library;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::find::{self, Found};
use super::model;
use crate::ast::{
    CreateRoutine, DropRoutine, Language, ObjectName, ParmStyle, Returns, RoutineBody, RoutineKind,
    SqlAccess, TypeSpec,
};
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;
use crate::exec::names as sqlnames;
use crate::pl::{self, Catalog, Role, SigParm, Signature};

fn unsupported(what: &str) -> SqlError {
    SqlError::Unsupported { what: what.into() }
}

/// `CREATE [OR REPLACE] PROCEDURE|FUNCTION`.
#[allow(clippy::too_many_lines)] // the clauses in order
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, r: &CreateRoutine) -> Result<()> {
    sqlnames::check_length(&r.name.name)?;
    let lib = ctx.library_for(txn, &r.name)?;
    let lib_name = ops::read_header(txn, lib)?.1.name;
    let routine_type = if r.kind == RoutineKind::Procedure { 1 } else { 2 };
    let func = r.kind == RoutineKind::Function;
    if func {
        check_function(r)?;
    }
    let o = &r.options;
    if let Some(s) = &o.specific
        && let Some(ss) = &s.schema
        && Some(ss) != r.name.schema.as_ref()
    {
        return Err(SqlError::Coded { code: -20_158, state: "42882", what: "the specific name's schema is not the routine's".into() });
    }
    let language = o.language.unwrap_or(if o.is_external { Language::Rpgle } else { Language::Sql });
    if o.is_external && language == Language::Sql {
        return Err(SqlError::Coded { code: -628, state: "42613", what: "LANGUAGE SQL and EXTERNAL together".into() });
    }
    match language {
        Language::C | Language::Cobol | Language::Cobolle | Language::Pli | Language::Rpg => {
            return Err(unsupported(&format!("external routines in LANGUAGE {}: QS/400 has no such compiler", language.word())));
        }
        Language::Java => return Err(unsupported("LANGUAGE JAVA routines")),
        _ => {}
    }
    let style = match (language, o.style) {
        (_, Some(ParmStyle::Db2General | ParmStyle::Java)) => return Err(unsupported("PARAMETER STYLE DB2GENERAL and JAVA")),
        (_, Some(ParmStyle::Db2Sql)) => return Err(unsupported("PARAMETER STYLE DB2SQL")),
        (Language::Sql, Some(_)) => {
            return Err(SqlError::Coded { code: -628, state: "42613", what: "PARAMETER STYLE on an SQL routine".into() });
        }
        (Language::Sql, None) => None,
        (_, s) => Some(s.unwrap_or(ParmStyle::Sql)),
    };
    // A procedure is one of its name and number of parameters.
    let existing = find::in_library(txn, lib)?;
    let same: Vec<&Found> = existing
        .iter()
        .filter(|f| f.routine.def.routine_type == routine_type && f.routine.def.name == r.name.name.value)
        .filter(|f| if func { same_types(f, &r.parms) } else { usize::from(f.routine.def.parm_count) == r.parms.len() })
        .collect();
    if let Some(old) = same.first() {
        if !r.replace {
            return Err(SqlError::Coded {
                code: -454,
                state: "42723",
                what: format!("a routine {} with the same signature is already in {}", r.name.name.value, old.schema),
            });
        }
        let old = (*old).clone();
        remove(txn, ctx, &old)?;
    }
    let specific = match &o.specific {
        Some(s) => s.name.value.clone(),
        None => specific_name(txn, lib, &r.name.name.value)?,
    };
    if names::find(txn, lib, Space::Specific, &specific)?.is_some_and(|t| ops::read_header(txn, t.id).is_ok()) {
        return Err(SqlError::Exists { name: specific, kind: "specific name" });
    }
    let mut parms = Vec::new();
    for (i, p) in r.parms.iter().enumerate() {
        let Some(n) = &p.name else {
            if language == Language::Sql {
                return Err(SqlError::Coded { code: -104, state: "42601", what: "an SQL routine's parameters need names".into() });
            }
            parms.push(ParmDef { ordinal: ordinal(i), mode: model::mode_code(p.mode), role: 0, name: String::new(), ty: pl::types::type_text(&p.ty), default: p.default.clone() });
            continue;
        };
        if r.parms[..i].iter().any(|q| q.name.as_ref() == Some(n)) {
            return Err(SqlError::Coded { code: -590, state: "42734", what: format!("parameter {} is named twice", n.value) });
        }
        parms.push(ParmDef { ordinal: ordinal(i), mode: model::mode_code(p.mode), role: 0, name: n.value.clone(), ty: pl::types::type_text(&p.ty), default: p.default.clone() });
    }
    if let Some(Returns::Scalar(t, _)) = &r.returns {
        // A function's result follows its parameters (`ROW_TYPE` C).
        parms.push(ParmDef { ordinal: ordinal(r.parms.len()), mode: 2, role: 2, name: String::new(), ty: pl::types::type_text(t), default: None });
    }
    if let Some(Returns::Table(cols)) = &r.returns {
        for (i, (n, t)) in cols.iter().enumerate() {
            parms.push(ParmDef { ordinal: ordinal(r.parms.len() + i), mode: 2, role: 1, name: n.value.clone(), ty: pl::types::type_text(t), default: None });
        }
    }
    let mut flags = 0u16;
    for (on, f) in [
        (o.deterministic == Some(true), flag::DETERMINISTIC),
        (o.commit_on_return, flag::COMMIT_ON_RETURN),
        (o.program_sub, flag::PROGRAM_SUB),
        (o.null_input, flag::NULL_INPUT),
        (o.no_external_action, flag::NO_EXTERNAL_ACTION),
        (o.not_fenced, flag::NOT_FENCED),
        (o.parallel, flag::PARALLEL),
        (o.final_call, flag::FINAL_CALL),
        (o.dbinfo, flag::DBINFO),
        (o.new_savepoint_level, flag::NEW_SAVEPOINT_LEVEL),
        (o.default_registers, flag::DEFAULT_REGISTERS),
    ] {
        if on {
            flags |= f;
        }
    }
    let mut def = RoutineDef {
        routine_type,
        language: model::language_code(language),
        style: style.map_or(0, model::style_code),
        access: model::access_code(o.access.unwrap_or(if func { SqlAccess::Reads } else { SqlAccess::Modifies })),
        body: match r.body {
            RoutineBody::Sql(_) => 1,
            RoutineBody::External => 2,
            RoutineBody::Sourced(..) => 3,
        },
        function_type: match &r.returns {
            Some(Returns::Table(_)) => 2,
            _ => u8::from(func),
        },
        flags,
        result_sets: o.result_sets,
        parm_count: u16::try_from(r.parms.len()).map_err(|_| unsupported("more than 65535 parameters"))?,
        scratchpad: o.scratchpad.unwrap_or(0),
        created: ctx.clock(),
        specific: specific.clone(),
        name: r.name.name.value.clone(),
        text: r.text.clone(),
        body_text: if matches!(r.body, RoutineBody::Sql(_)) { body_definition(&r.body_text) } else { String::new() },
        ..RoutineDef::default()
    };
    let routine_options = o.set_options.clone();
    let (owner, deps) = match &r.body {
        _ if func => {
            let program = program_name_of(txn, lib, &r.name.name, types::SRVPGM)?;
            def.program_library = lib_name.to_string();
            def.program = program.to_string();
            // IBM's procedure in the service program: NAME_1, or NAME for a
            // sourced function.
            def.entry = format!("{}_1", r.name.name.value);
            if let RoutineBody::Sourced(src, _) = &r.body {
                def.sourced = src.to_string();
                def.entry.clone_from(&r.name.name.value);
                def.body_text.clear();
            }
            (holder(txn, ctx, lib, program, &r.text)?, Vec::new())
        }
        RoutineBody::Sql(body) => {
            if r.parms.iter().any(|p| p.ty.ccsid.is_some() || p.ty.bit_data) {
                return Err(unsupported("SQL routine parameters with a CCSID or FOR BIT DATA"));
            }
            let program = program_name(txn, lib, &r.name.name)?;
            def.program_library = lib_name.to_string();
            def.program = program.to_string();
            let commit = commit_option(&routine_options)?;
            let sig = Signature {
                program: program.to_string(),
                routine: r.name.name.value.clone(),
                parms: r
                    .parms
                    .iter()
                    .map(|p| SigParm { name: p.name.as_ref().map(|n| n.value.clone()).unwrap_or_default(), mode: p.mode, ty: p.ty.clone() })
                    .collect(),
                role: Role::Procedure,
                naming: ctx.naming,
                commit,
                groups: Vec::new(),
            };
            let mut cat = Cat { txn: &mut *txn, ctx: &mut *ctx, deps: Vec::new() };
            let qir = pl::compile(&sig, body, &mut cat)?;
            let deps = std::mem::take(&mut cat.deps);
            let id = make_program(txn, ctx, lib, program, &qir, &r.text)?;
            (id, deps)
        }
        RoutineBody::External => {
            let (plib, pname, entry) = external_name(o.external.as_deref(), &r.name)?;
            if o.program_sub && entry.is_none() {
                return Err(SqlError::Coded { code: -20_210, state: "42904", what: "PROGRAM TYPE SUB needs a service program procedure".into() });
            }
            def.program_library.clone_from(&plib);
            def.program.clone_from(&pname);
            def.entry = entry.clone().unwrap_or_default();
            let ty = if entry.is_some() { types::SRVPGM } else { types::PGM };
            let owner = if let Some(o) = external_object(txn, ctx, &plib, &pname, ty)? {
                o
            } else {
                // A program not made yet: the definition waits in a holder
                // service program of the routine's own, as a function's does.
                let h = program_name_of(txn, lib, &r.name.name, types::SRVPGM)?;
                ctx.warning = Some(no_program(&plib, &pname));
                holder(txn, ctx, lib, h, &r.text)?
            };
            (owner, Vec::new())
        }
        RoutineBody::Sourced(..) => return Err(unsupported("sourced functions")),
    };
    let routine = Routine { def, parms, options: routine_options };
    let mut info = sqlinf::load(txn, owner)?.unwrap_or_else(|| SqlInfo::new(Kind::Procedure));
    Routine::replace(&mut info, &specific, Some(&routine))?;
    for d in deps {
        info.push(tag::DEPENDENCY, d.encode());
    }
    sqlinf::save(txn, owner, &info, ctx.clock())?;
    let system = ops::read_header(txn, owner)?.1.name;
    names::put(txn, lib, Space::Specific, &specific, Target { id: owner, system })?;
    if func {
        super::udf::remember(r, &sqlnames::schema_name(txn, lib));
    }
    Ok(())
}

fn ordinal(i: usize) -> u16 {
    u16::try_from(i + 1).unwrap_or(u16::MAX)
}

/// The body as the catalog keeps it (`ROUTINE_DEFINITION`): its tokens
/// one blank apart, its lines as written without their indentation,
/// and a blank at the end, as IBM shows it.
pub(crate) fn body_definition(text: &str) -> String {
    let Ok(toks) = crate::lex::tokenize(text) else { return text.to_string() };
    let mut out = String::new();
    let toks: Vec<_> = toks.into_iter().filter(|t| !matches!(t.tok, crate::lex::Tok::End)).collect();
    for (i, t) in toks.iter().enumerate() {
        crate::pl::rewrite::render(&t.tok, &mut out);
        let next_line = toks.get(i + 1).is_some_and(|n| n.line > t.line);
        out.push(if next_line { '\n' } else { ' ' });
    }
    out
}

/// `SET OPTION COMMIT`.
pub(crate) fn commit_option(opts: &[(String, String)]) -> Result<Option<db::cmt::LckLvl>> {
    for (k, v) in opts {
        match k.as_str() {
            "COMMIT" => return Ok(crate::embed::commit_level(v)),
            "DATFMT" | "DATSEP" | "TIMFMT" | "TIMSEP" | "DECMPT" | "SRTSEQ" | "LANGID" | "DLYPRP" | "USRPRF"
            | "DYNUSRPRF" | "ALWCPYDTA" | "CLOSQLCSR" | "DBGVIEW" | "OUTPUT" | "TGTRLS" | "NAMING" | "SQLPATH"
            | "ALWBLK" | "DECRESULT" | "SQLCURRULE" | "CONACC" | "SYSTIME" | "DFTRDBCOL" | "EXTIND" | "COMPILEOPT"
            | "DBGENCKEY" | "DECFLTRND" | "LANGIDA" => {}
            other => return Err(unsupported(&format!("SET OPTION {other}"))),
        }
    }
    Ok(None)
}

/// A specific name for a routine of `name` in `lib`: the name itself
/// while it is free, else five characters and a number.
fn specific_name(txn: &mut Txn<'_>, lib: ObjectId, name: &str) -> Result<String> {
    let taken = |txn: &mut Txn<'_>, s: &str| -> Result<bool> { Ok(names::find(txn, lib, Space::Specific, s)?.is_some_and(|t| ops::read_header(txn, t.id).is_ok())) };
    if !taken(txn, name)? {
        return Ok(name.to_string());
    }
    let five: String = name.chars().take(5).collect();
    for n in 1..=99_999u32 {
        let s = format!("{five}{n:05}");
        if !taken(txn, &s)? {
            return Ok(s);
        }
    }
    Err(unsupported("a specific name: every one the name could have is taken"))
}

/// The program's name: the routine's when it is a system name no
/// program has, else a generated one.
pub(crate) fn program_name(txn: &mut Txn<'_>, lib: ObjectId, name: &crate::ast::Ident) -> Result<Name> {
    program_name_of(txn, lib, name, types::PGM)
}

/// The name of a new object of `ty` for a routine.
fn program_name_of(txn: &mut Txn<'_>, lib: ObjectId, name: &crate::ast::Ident, ty: u16) -> Result<Name> {
    let l = Library::load(txn, lib)?;
    if let Some(n) = sqlnames::direct(name)
        && l.lookup(txn, ty, n)?.is_none()
    {
        return Ok(n);
    }
    sqlnames::generate(txn, lib, ty, &name.value, &|_| false)
}

/// `EXTERNAL NAME`: library, program and service program procedure.
fn external_name(ext: Option<&str>, routine: &ObjectName) -> Result<(String, String, Option<String>)> {
    let text = match ext {
        Some(t) => t.trim().to_string(),
        None => routine.name.value.clone(),
    };
    let (obj, entry) = match text.split_once('(') {
        Some((o, rest)) => (o.to_string(), Some(rest.trim_end_matches(')').trim().to_string())),
        None => (text.clone(), None),
    };
    let (lib, pgm) = match obj.split_once(['/', '.']) {
        Some((l, p)) => (l.trim().to_ascii_uppercase(), p.trim().to_ascii_uppercase()),
        None => ("*LIBL".to_string(), obj.trim().to_ascii_uppercase()),
    };
    if Name::parse(&pgm).is_err() || (lib != "*LIBL" && Name::parse(&lib).is_err()) {
        return Err(SqlError::Coded { code: -113, state: "28000", what: format!("EXTERNAL NAME {text} is not valid") });
    }
    Ok((lib, pgm, entry))
}

/// The external program, found now.
fn external_object(txn: &mut Txn<'_>, ctx: &SqlContext, lib: &str, name: &str, ty: u16) -> Result<Option<ObjectId>> {
    let n = Name::parse(name).map_err(|e| SqlError::Coded { code: -113, state: "28000", what: e.to_string() })?;
    let libs: Vec<ObjectId> = if lib == "*LIBL" {
        let mut v: Vec<ObjectId> = ctx.names.curlib.into_iter().collect();
        v.extend(ctx.names.libl.iter().copied());
        v
    } else {
        let q = Library::load(txn, ctx.qsys)?;
        let ln = Name::parse(lib).map_err(|e| SqlError::Coded { code: -113, state: "28000", what: e.to_string() })?;
        q.lookup(txn, types::LIB, ln)?.into_iter().collect()
    };
    for l in libs {
        if let Some(id) = Library::load(txn, l)?.lookup(txn, ty, n)? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// The compiled module bound as a `*PGM` in `lib`.
pub(crate) fn make_program(txn: &mut Txn<'_>, ctx: &SqlContext, lib: ObjectId, name: Name, qir_text: &str, text: &str) -> Result<ObjectId> {
    make_object(txn, ctx, lib, name, qir_text, text, PgmKind::Pgm)
}

/// A module bound as a program or service program in `lib`.
fn make_object(txn: &mut Txn<'_>, ctx: &SqlContext, lib: ObjectId, name: Name, qir_text: &str, text: &str, kind: PgmKind) -> Result<ObjectId> {
    let fail = |e: &dyn std::fmt::Display| SqlError::Database { what: format!("the routine's program was not made: {e}"), msgid: Some("CPF5D12".into()) };
    let module = qir::parse(qir_text).map_err(|e| fail(&e))?;
    qir::verify(&module).map_err(|e| fail(&e))?;
    let create = CreateData { compiler: "SQL".into(), options: String::new(), src_file: String::new(), src_lib: String::new(), src_mbr: String::new() };
    let object = ModuleObject::compile(module, create, ctx.clock()).map_err(|e| fail(&e))?;
    let system = ops::read_header(txn, ctx.qsys)?.1.system;
    let c = IleCtx {
        qsys: ctx.qsys,
        names: ctx.names.clone(),
        now: ctx.clock(),
        owner: ctx.user.clone(),
        system: store::obj::header::text_of(&system),
    };
    let label: String = text.chars().take(50).collect();
    let l = Library::load(txn, lib)?;
    if let Some(old) = l.lookup(txn, types::MODULE, name)? {
        ops::delete(txn, JobHandle(0), old)?;
    }
    let m = ile::store::crtmod(txn, &c, lib, name, &object, "").map_err(|e| fail(&e))?;
    let mut options = BindOptions::new(&name.to_string(), kind);
    options.attr.actgrp = ActGrpAttr::Caller;
    let lib_name = ops::read_header(txn, lib)?.1.name;
    let spec = ObjSpec { lib: LibSpec::Name(lib_name), name };
    let (id, _) = ile::store::crtpgm(txn, &c, lib, &[spec], &[], options, &label).map_err(|e| fail(&e))?;
    ops::delete(txn, JobHandle(0), m)?;
    Ok(id)
}

/// The catalog the compiler asks while it compiles, over the store.
pub(crate) struct Cat<'a, 't> {
    pub(crate) txn: &'a mut Txn<'t>,
    pub(crate) ctx: &'a mut SqlContext,
    pub(crate) deps: Vec<db::sqlinf::body::Dependency>,
}

impl Catalog for Cat<'_, '_> {
    fn columns(&mut self, table: &ObjectName) -> Vec<String> {
        let Ok(t) = self.ctx.resolve_table(self.txn, table) else { return Vec::new() };
        if let Ok((_, h)) = ops::read_header(self.txn, t.id)
            && let Ok((_, lh)) = ops::read_header(self.txn, h.library)
        {
            self.deps.push(db::sqlinf::body::Dependency { type_code: types::FILE, library: lh.name, name: h.name });
        }
        let mut out = Vec::new();
        for f in &t.format.format.fields {
            out.push(f.name.to_string());
            if let Some(a) = &f.alias {
                out.push(a.clone());
            }
        }
        out
    }

    fn describe(&mut self, query: &str) -> Result<Vec<(String, TypeSpec)>> {
        let p = crate::exec::prepare(query, self.ctx.naming)?;
        let mut out = Vec::new();
        for e in crate::embed::describe::entries(self.txn, self.ctx, &p.statement, false) {
            let (c, n) = (e.col, e.name);
            let t = model::type_of_sqltype(c.base, c.len).ok_or_else(|| unsupported(&format!("a FOR loop over column {n} of SQL type {}", c.base)))?;
            out.push((n, t));
        }
        Ok(out)
    }

    fn array_type(&mut self, name: &ObjectName) -> Result<Option<(TypeSpec, u64)>> {
        let Some((_, u)) = crate::exec::udt::find(self.txn, self.ctx, name)? else { return Ok(None) };
        if u.metatype != b'A' {
            // BLOCKED(sql): distinct types need strong typing in the engine.
            return Err(unsupported(&format!("a variable of distinct type {name}")));
        }
        Ok(Some((crate::pl::types::parse_type(&u.source)?, u.cardinality)))
    }
}

/// `DROP PROCEDURE|FUNCTION|SPECIFIC ...`.
pub fn drop(txn: &mut Txn<'_>, ctx: &mut SqlContext, d: &DropRoutine) -> Result<()> {
    let one = the_one(txn, ctx, d)?;
    remove(txn, ctx, &one)
}

/// The one routine a `DROP` names: `SQL0204` for none, `SQL0476` for
/// more than one.
pub(super) fn the_one(txn: &mut Txn<'_>, ctx: &mut SqlContext, d: &DropRoutine) -> Result<Found> {
    let routine_type = if d.kind == RoutineKind::Procedure { 1 } else { 2 };
    let not_found = || SqlError::ObjectNotFound { name: d.name.to_string(), kind: if routine_type == 1 { "procedure" } else { "function" } };
    let found = if d.specific {
        find::specific(txn, ctx, &d.name)?.into_iter().filter(|f| f.routine.def.routine_type == routine_type).collect()
    } else {
        let mut v = find::named(txn, ctx, &d.name, routine_type)?;
        if let Some(sig) = &d.signature {
            v.retain(|f| signature_matches(f, sig));
        }
        v
    };
    match found.as_slice() {
        [] => Err(not_found()),
        [one] => Ok(one.clone()),
        _ => Err(SqlError::Coded {
            code: -476,
            state: "42725",
            what: format!("{} names more than one routine: give the signature or the specific name", d.name),
        }),
    }
}

/// Whether a routine's parameters are of the types given.
fn signature_matches(f: &Found, sig: &[TypeSpec]) -> bool {
    let ps: Vec<&ParmDef> = f.routine.parms.iter().filter(|p| p.role == 0).collect();
    ps.len() == sig.len()
        && ps.iter().zip(sig).all(|(p, t)| pl::types::parse_type(&p.ty).is_ok_and(|pt| pt.data_type == t.data_type))
}

/// Drop a routine: an SQL routine's program goes with it; an external
/// one's program only loses the routine's attributes.
/// `SQL7909`: the routine's program is not there (yet).
fn no_program(lib: &str, pgm: &str) -> crate::sqlca::Warning {
    crate::sqlca::Warning { sqlcode: 7909, sqlstate: "01660".into(), text: format!("{lib}/{pgm}") }
}

pub(super) fn remove(txn: &mut Txn<'_>, ctx: &mut SqlContext, f: &Found) -> Result<()> {
    if f.routine.def.routine_type == 2 {
        super::udf::forget(&f.routine.def.name, &f.schema, f.routine.def.parm_count.into());
    }
    names::remove(txn, f.lib, Space::Specific, &f.routine.def.specific)?;
    if f.routine.def.body == 1 || f.routine.def.routine_type == 2 {
        sqlinf::clear(txn, ctx.job, f.owner)?;
        ops::delete(txn, ctx.job, f.owner)?;
        return Ok(());
    }
    let Some(mut info) = sqlinf::load(txn, f.owner)? else { return Ok(()) };
    Routine::replace(&mut info, &f.routine.def.specific, None)?;
    if info.entries.iter().any(|e| e.tag == tag::ROUTINE) {
        sqlinf::save(txn, f.owner, &info, ctx.clock())?;
    } else {
        sqlinf::clear(txn, ctx.job, f.owner)?;
        // A holder made for a program not there yet goes with it.
        let (_, h) = ops::read_header(txn, f.owner)?;
        if f.owner.type_code == types::SRVPGM && h.name.to_string() != f.routine.def.program {
            ops::delete(txn, ctx.job, f.owner)?;
            let ty = if f.routine.def.entry.is_empty() { types::PGM } else { types::SRVPGM };
            if external_object(txn, ctx, &f.routine.def.program_library, &f.routine.def.program, ty)?.is_none() {
                ctx.warning = Some(no_program(&f.routine.def.program_library, &f.routine.def.program));
            }
        }
    }
    Ok(())
}


/// What `CREATE FUNCTION` takes: SQL scalar functions whose bodies the
/// database runs (`udf`) and sourced functions.
fn check_function(r: &CreateRoutine) -> Result<()> {
    match (&r.returns, &r.body) {
        (_, RoutineBody::External) => {
            // BLOCKED(sql): an external function runs a program inside the
            // query, and programs run in the job, not the database.
            Err(unsupported("external functions"))
        }
        (_, RoutineBody::Sql(b)) => super::udf::check_body(b),
        _ => Ok(()),
    }
}

/// Whether a function's parameters are of the same types (lengths
/// aside), its signature.
fn same_types(f: &Found, parms: &[crate::ast::Parm]) -> bool {
    let mine: Vec<&ParmDef> = f.routine.parms.iter().filter(|p| p.role == 0).collect();
    mine.len() == parms.len()
        && mine.iter().zip(parms).all(|(p, q)| {
            pl::types::parse_type(&p.ty).is_ok_and(|t| std::mem::discriminant(&t.data_type) == std::mem::discriminant(&q.ty.data_type))
        })
}

/// The service program that holds a function's definition: its one
/// procedure does nothing, as the database runs the body itself.
fn holder(txn: &mut Txn<'_>, ctx: &SqlContext, lib: ObjectId, name: Name, text: &str) -> Result<ObjectId> {
    let n = name.to_string();
    let qir = format!("module {n} lang(QIR) ccsid(37) actgrp(*CALLER) nomain\nproc {n}() export {{\nL0:\n  ret\n}}\n");
    make_object(txn, ctx, lib, name, &qir, text, PgmKind::SrvPgm)
}
