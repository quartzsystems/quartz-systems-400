// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object commands on the store: `ALCOBJ`, `DLCOBJ`, `WRKOBJLCK` on the
//! lock table, `CHGLIB`, and `DSPOVR` over the job's override tables.

use std::fmt::Write as _;

use cmd::Value;
use cmd::block::Parms;
use stg::proto::CmdReply;
use store::obj::library::{LibType, Library};
use store::obj::{JobHandle, LockState, ObjectId, ops, types};
use store::txn::Txn;

use super::direct::{failed, in_store, lib_or, plain};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "ALCOBJ" => alcobj(system, parms, true),
        "DLCOBJ" => alcobj(system, parms, false),
        "WRKOBJLCK" => wrkobjlck(system, parms),
        "CHGLIB" => chglib(system, parms),
        "DSPOVR" => Ok(dspovr(system, parms)),
        "UPDSRVPGM" => Ok(updsrvpgm(system, parms)),
        "RMVBNDDIRE" => rmvbnddire(system, parms),
        _ => return None,
    };
    Some(match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    })
}

fn job_handle(system: &System) -> JobHandle {
    JobHandle(
        system
            .jobs
            .get(system.current_job)
            .map_or(u64::from(system.current_job), |j| j.handle),
    )
}

fn lock_state(s: &str) -> Result<LockState, String> {
    Ok(match s {
        "*EXCL" => LockState::Exclusive,
        "*EXCLRD" => LockState::ExclusiveRead,
        "*SHRUPD" => LockState::SharedUpdate,
        "*SHRNUP" => LockState::SharedNoUpdate,
        "*SHRRD" => LockState::SharedRead,
        _ => return Err(format!("CPD0084 lock state {s} is not valid")),
    })
}

/// One `OBJ` element: `(name, library, type, lock state)`.
struct ObjLock {
    name: String,
    lib: String,
    ty: String,
    state: LockState,
}

fn obj_list(parms: &Parms) -> Result<Vec<ObjLock>, String> {
    let mut out = Vec::new();
    for item in parms.list("OBJ") {
        let Value::Elems(elems) = item else {
            return Err("CPD0084 OBJ: (object type lock-state) is needed".into());
        };
        let part = |i: usize| elems.get(i).cloned().flatten();
        let (name, lib) = match part(0) {
            Some(Value::Qual(q)) => (
                q.first().map(Value::source).unwrap_or_default(),
                q.get(1).map(Value::source).unwrap_or_default(),
            ),
            Some(v) => (v.source(), String::new()),
            None => return Err("CPD0084 OBJ: an object name is needed".into()),
        };
        let ty = part(1).map(|v| v.source().to_ascii_uppercase()).unwrap_or_default();
        let state = part(2).map_or_else(|| "*EXCL".to_string(), |v| v.source().to_ascii_uppercase());
        out.push(ObjLock {
            name: name.to_ascii_uppercase(),
            lib: lib.to_ascii_uppercase(),
            ty,
            state: lock_state(&state)?,
        });
    }
    Ok(out)
}

fn resolve(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    o: &ObjLock,
) -> Result<ObjectId, String> {
    let code = types::type_by_name(&o.ty)
        .map(|t| t.code)
        .ok_or_else(|| format!("CPD0084 object type {} is not valid", o.ty))?;
    wm::obj::find(txn, qsys, names, &lib_or(&o.lib), &o.name, code)
        .map_err(|_| format!("CPF9801 {} {}/{} not found", o.ty, lib_or(&o.lib), o.name))
}

/// `ALCOBJ` and `DLCOBJ`: the job's locks on the objects named. A lock
/// another job holds in conflict is `CPF1002` at once (`WAIT` is not
/// waited out: the service answers every job on one thread).
fn alcobj(system: &mut System, parms: &Parms, allocate: bool) -> Result<String, String> {
    let objs = obj_list(parms)?;
    let job = job_handle(system);
    in_store(system, |txn, qsys, names| {
        let mut taken = Vec::new();
        for o in &objs {
            let id = resolve(txn, qsys, names, o)?;
            if allocate {
                if let Err(held) = txn.store().locks_mut().lock(job, id, o.state) {
                    for (id, st) in taken {
                        txn.store().locks_mut().unlock(job, id, st);
                    }
                    return Err(format!(
                        "CPF1002 cannot allocate {} {}: held {} by job {}",
                        o.ty,
                        o.name,
                        held.state.name(),
                        held.job.0
                    ));
                }
                taken.push((id, o.state));
            } else {
                txn.store().locks_mut().unlock(job, id, o.state);
            }
        }
        Ok(format!(
            "{} object(s) {}.\n",
            objs.len(),
            if allocate { "allocated" } else { "deallocated" }
        ))
    })
}

/// `WRKOBJLCK`: who holds the object.
fn wrkobjlck(system: &mut System, parms: &Parms) -> Result<String, String> {
    let (name, lib) = parms.qual("OBJ");
    let o = ObjLock {
        name: name.to_ascii_uppercase(),
        lib: lib.to_ascii_uppercase(),
        ty: parms.name("OBJTYPE"),
        state: LockState::SharedRead,
    };
    let jobs = system.jobs.all();
    in_store(system, |txn, qsys, names| {
        let id = resolve(txn, qsys, names, &o)?;
        let holders = txn.store().locks().holders(id);
        let mut out = format!(
            "Object {}/{} type {}:\nJob         User        Number  Lock     Status\n",
            lib_or(&o.lib),
            o.name,
            o.ty
        );
        if holders.is_empty() {
            out.push_str("  (no locks)\n");
        }
        for h in holders {
            let (jn, ju, nbr) = jobs
                .iter()
                .find(|j| j.handle == h.job.0)
                .map_or(("*UNKNOWN".to_string(), String::new(), 0), |j| {
                    (j.name.clone(), j.user.clone(), j.number)
                });
            let _ = writeln!(out, "{jn:<10}  {ju:<10}  {nbr:06}  {:<8} HELD", h.state.name());
        }
        Ok(out)
    })
}

/// `CHGLIB`: text, `TYPE` and `CRTAUT`.
fn chglib(system: &mut System, parms: &Parms) -> Result<String, String> {
    let lib = parms.name("LIB");
    let text = parms.specified("TEXT").then(|| parms.text("TEXT"));
    let ty = parms.specified("TYPE").then(|| parms.name("TYPE"));
    let crtaut = parms.specified("CRTAUT").then(|| parms.name("CRTAUT"));
    let now = now_unix();
    in_store(system, |txn, qsys, _| {
        let id = wm::obj::library(txn, qsys, &lib).map_err(|e| format!("CPF2110 {e}"))?;
        if let Some(t) = text {
            let t = if t == "*BLANK" { "" } else { &t };
            ops::change_text(txn, id, t, now).map_err(|e| e.to_string())?;
        }
        if ty.is_some() || crtaut.is_some() {
            let mut l = Library::load(txn, id).map_err(|e| e.to_string())?;
            if let Some(t) = ty {
                l.attr.lib_type = match t.as_str() {
                    "*PROD" | "*SAME" => l.attr.lib_type,
                    "*TEST" => LibType::Test,
                    v => return Err(format!("CPD0084 TYPE({v}) is not valid")),
                };
                if t == "*PROD" {
                    l.attr.lib_type = LibType::Prod;
                }
            }
            if let Some(a) = crtaut.filter(|a| a != "*SAME") {
                l.attr.create_authority =
                    store::obj::header::text(&a).map_err(|e| format!("CPD0084 CRTAUT({a}): {e}"))?;
            }
            l.save(txn).map_err(|e| e.to_string())?;
        }
        Ok(format!("Library {lib} changed.\n"))
    })
}

/// `DSPOVR`: the job's database and printer file overrides.
fn dspovr(system: &mut System, parms: &Parms) -> String {
    let only = parms.name("FILE");
    let only = (!only.is_empty() && only != "*ALL").then_some(only);
    let mut out = String::from("File        Type    Level  Override\n");
    let prt: Vec<(String, u32, String)> = system
        .svc
        .prt_overrides
        .iter()
        .map(|o| (o.file.clone(), o.level, o.line.clone()))
        .collect();
    let mut n = 0;
    if let Ok((_, ctx)) = crate::svc::db::db(system) {
        for o in ctx.session.overrides() {
            let f = o.file.to_string();
            if only.as_ref().is_some_and(|w| *w != f) {
                continue;
            }
            n += 1;
            let mut what = Vec::new();
            if let Some(t) = &o.tofile {
                what.push(format!("TOFILE({}/{})", t.library, t.file));
            }
            if let Some(m) = &o.member {
                what.push(format!("MBR({m})"));
            }
            if let Some(s) = o.share {
                what.push(format!("SHARE({})", if s { "*YES" } else { "*NO" }));
            }
            if let Some(l) = o.lvlchk {
                what.push(format!("LVLCHK({})", if l { "*YES" } else { "*NO" }));
            }
            if o.secure {
                what.push("SECURE(*YES)".into());
            }
            if o.scope != db::odp::ovrdbf::DFTACTGRP {
                what.push(format!("OVRSCOPE({})", o.scope));
            }
            let _ = writeln!(out, "{f:<10}  *DBF    {:>5}  {}", o.level, what.join(" "));
        }
    }
    for (f, level, line) in prt {
        if only.as_ref().is_some_and(|w| *w != f) {
            continue;
        }
        n += 1;
        let rest = line.split_once(' ').map_or("", |(_, r)| r);
        let _ = writeln!(out, "{f:<10}  *PRTF   {level:>5}  {rest}");
    }
    if n == 0 {
        out.push_str("  (no overrides in effect)\n");
    }
    out
}

/// `UPDSRVPGM`: the `updpgm` word takes a service program too.
fn updsrvpgm(system: &mut System, parms: &Parms) -> String {
    let (n, l) = parms.qual("SRVPGM");
    let modules: Vec<String> = parms
        .list("MODULE")
        .iter()
        .map(|v| match v {
            Value::Qual(q) => format!(
                "{}/{}",
                q.get(1).map_or_else(|| "*LIBL".to_string(), Value::source),
                q.first().map(Value::source).unwrap_or_default()
            ),
            other => other.source(),
        })
        .collect();
    super::direct::old(
        system,
        &format!("updpgm PGM({}/{n}) MODULE({})", lib_or(&l), modules.join(" ")),
    )
}

/// `RMVBNDDIRE BNDDIR(lib/dir) OBJ((lib/name *SRVPGM|*MODULE) ...)`.
fn rmvbnddire(system: &mut System, parms: &Parms) -> Result<String, String> {
    let q = parms.qual("BNDDIR");
    let mut entries: Vec<(String, Option<String>, ile::BndEntryKind)> = Vec::new();
    for item in parms.list("OBJ") {
        let (object, ty) = match item {
            Value::Elems(e) => (e.first().cloned().flatten(), e.get(1).cloned().flatten()),
            other => (Some(other.clone()), None),
        };
        let (name, lib) = match object {
            Some(Value::Qual(qn)) => (
                qn.first().map(Value::source).unwrap_or_default(),
                qn.get(1).map(Value::source).filter(|l| l != "*LIBL"),
            ),
            Some(v) => (v.source(), None),
            None => return Err("CPD0084 OBJ: an object name is needed".into()),
        };
        let kind = match ty.map(|v| v.source().to_ascii_uppercase()).as_deref() {
            None | Some("*SRVPGM") => ile::BndEntryKind::SrvPgm,
            Some("*MODULE") => ile::BndEntryKind::Module,
            Some(t) => return Err(format!("CPD0084 OBJ: type {t} is not *SRVPGM or *MODULE")),
        };
        entries.push((name.to_ascii_uppercase(), lib.map(|l| l.to_ascii_uppercase()), kind));
    }
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib_or(&q.1), &q.0, types::BNDDIR)
            .map_err(|_| format!("CPF9801 binding directory {}/{} not found", lib_or(&q.1), q.0))?;
        let mut n = 0;
        for (name, lib, kind) in &entries {
            n += ile::store::rmvbnddire(txn, id, name, lib.as_deref(), *kind)
                .map_err(|e| format!("CPF5D01 {e}"))?;
        }
        Ok(format!("{n} entr{} removed from binding directory {}.\n", if n == 1 { "y" } else { "ies" }, q.0))
    })
}
