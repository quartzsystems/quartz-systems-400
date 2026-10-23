// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debugger's storage requests (380-399): a program's modules with
//! their source and statement lines for Display Module Source, and
//! `STRSRVJOB`: which job services which, with a mailbox each way. The
//! servicing job posts orders (`STRDBG`, breakpoints, resume, `EVAL`)
//! that the serviced job takes at its next check (a call, a break
//! poll, while stopped); the serviced job posts its stops and answers
//! back. The table lives as long as the service.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use ile::ModuleObject;
use qsname::Name;
use sec::profile::spcaut::SpecialAuth;
use stg::JobInfo;
use stg::proto::{DbgEvent, DbgModule, DbgOrder, DbgProgram};
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::System;
use crate::svc::db::with_txn;

/// Who services whom, and what waits for each.
#[derive(Default)]
struct Table {
    /// Servicing job number to serviced job.
    serving: BTreeMap<u32, JobInfo>,
    /// Orders by serviced job number.
    orders: BTreeMap<u32, VecDeque<DbgOrder>>,
    /// Events by servicing job number.
    events: BTreeMap<u32, VecDeque<DbgEvent>>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// `number/user/name`.
fn qualified(j: &JobInfo) -> String {
    format!("{:06}/{}/{}", j.number, j.user, j.name)
}

/// `STRSRVJOB`: `me` services the job named. The same user may service
/// its own jobs; another user's needs `*SERVICE` or `*ALLOBJ`.
pub fn start(
    system: &System,
    me: &JobInfo,
    special: SpecialAuth,
    job: &str,
) -> Result<String, String> {
    let parts: Vec<String> = job
        .split('/')
        .map(|p| p.trim().to_ascii_uppercase())
        .collect();
    let (number, user, name) = match parts.as_slice() {
        [n, u, j] => (n.parse::<u32>().ok(), Some(u.clone()), j.clone()),
        [u, j] => (None, Some(u.clone()), j.clone()),
        [j] => (None, None, j.clone()),
        _ => return Err(format!("CPF3520 Job {job} not found.")),
    };
    let found: Vec<JobInfo> = system
        .jobs
        .all()
        .into_iter()
        .filter(|j| {
            j.name == name
                && user.as_ref().is_none_or(|u| *u == j.user)
                && number.is_none_or(|n| n == j.number)
        })
        .collect();
    let target = match found.as_slice() {
        [] => return Err(format!("CPF3520 Job {job} not found.")),
        [one] => one.clone(),
        _ => return Err(format!("CPF1071 More than one job named {job}; give its number.")),
    };
    if target.number == me.number {
        return Err("CPF3501 A job cannot service itself.".into());
    }
    if target.user != me.user && !special.has(SpecialAuth::SERVICE) && !special.has(SpecialAuth::ALLOBJ) {
        return Err(format!(
            "CPF3502 Not authorized to service job {}.",
            qualified(&target)
        ));
    }
    let q = qualified(&target);
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        if t.serving.values().any(|j| j.number == target.number)
            && t.serving.get(&me.number).is_none_or(|j| j.number != target.number)
        {
            return Err(format!("CPF3503 Job {q} is already being serviced."));
        }
        t.serving.insert(me.number, target.clone());
        t.orders.entry(target.number).or_default();
        Ok(q.clone())
    })
}

/// `ENDSRVJOB`: the serviced job's name.
pub fn end(me: u32) -> Result<String, String> {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        let j = t
            .serving
            .remove(&me)
            .ok_or_else(|| "CPF3504 No job is being serviced.".to_string())?;
        // A stopped job must not wait for a servicer that left.
        t.orders.entry(j.number).or_default().push_back(DbgOrder::End);
        t.events.remove(&me);
        Ok(qualified(&j))
    })
}

/// The job `me` services.
pub fn serving(me: u32) -> String {
    TABLE.with(|t| t.borrow().serving.get(&me).map(qualified).unwrap_or_default())
}

/// An order from `me` for the job it services.
pub fn order(me: u32, o: DbgOrder) -> Result<(), String> {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        let n = t
            .serving
            .get(&me)
            .map(|j| j.number)
            .ok_or_else(|| "CPF3504 No job is being serviced.".to_string())?;
        t.orders.entry(n).or_default().push_back(o);
        Ok(())
    })
}

/// The orders waiting for `me`.
pub fn orders(me: u32) -> Vec<DbgOrder> {
    TABLE.with(|t| {
        t.borrow_mut()
            .orders
            .get_mut(&me)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    })
}

/// News from `me` for the job servicing it; refused when none does.
pub fn event(me: u32, e: DbgEvent) -> Result<(), String> {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        let by = t
            .serving
            .iter()
            .find(|(_, j)| j.number == me)
            .map(|(k, _)| *k)
            .ok_or_else(|| "CPF3504 The job is not being serviced.".to_string())?;
        t.events.entry(by).or_default().push_back(e);
        Ok(())
    })
}

/// The news waiting for `me`.
pub fn events(me: u32) -> Vec<DbgEvent> {
    TABLE.with(|t| {
        t.borrow_mut()
            .events
            .get_mut(&me)
            .map(|q| q.drain(..).collect())
            .unwrap_or_default()
    })
}

/// A program's library and object, `*PGM` first, then `*SRVPGM`.
fn locate(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
) -> Result<(String, ObjectId), String> {
    let n = Name::parse(name).map_err(|e| format!("CPD0078 {e}"))?;
    let names = crate::svc::ile::name_context(txn, qsys);
    let libs: Vec<ObjectId> = match lib {
        "" | "*LIBL" => names.libl.clone(),
        "*CURLIB" => names.curlib.into_iter().collect(),
        l => vec![
            ops::find_library(txn, qsys, Name::parse(l).map_err(|e| format!("CPD0078 {e}"))?)
                .map_err(|_| format!("CPF2110 Library {l} not found."))?
                .id,
        ],
    };
    for ty in [types::PGM, types::SRVPGM] {
        for l in &libs {
            let lb = Library::load(txn, *l).map_err(|e| e.to_string())?;
            if let Some(id) = lb.lookup(txn, ty, n).map_err(|e| e.to_string())? {
                let lname = if *l == qsys {
                    "QSYS".to_string()
                } else {
                    ops::describe(txn, *l)
                        .map(|h| h.name.to_string())
                        .unwrap_or_default()
                };
                return Ok((lname, id));
            }
        }
    }
    let shown = if lib.is_empty() { "*LIBL" } else { lib };
    Err(format!("CPF9801 Program {name} in library {shown} not found."))
}

/// A program's modules for Display Module Source.
pub fn program(system: &mut System, lib: &str, name: &str) -> Result<DbgProgram, String> {
    let (lib, kind, mods) = {
        let s = system.storage.as_mut().ok_or("object store not open")?;
        let qsys = s.qsys.ok_or("object store not initialized")?;
        with_txn(&mut s.store, |txn| {
            let (lib, id) = locate(txn, qsys, lib, name)?;
            let kind = if id.type_code == types::SRVPGM {
                "*SRVPGM"
            } else {
                "*PGM"
            };
            let p = ile::store::load_program(txn, id).map_err(|e| e.to_string())?;
            let mods: Vec<ModuleObject> = p.modules.into_iter().map(|b| b.module).collect();
            Ok((lib, kind.to_string(), mods))
        })?
    };
    let mut modules = Vec::new();
    for m in mods {
        let lines = match &m.source {
            Some(l) => l.clone(),
            None if !m.create.src_file.is_empty() => super::pdm::read(
                system,
                &m.create.src_lib,
                &m.create.src_file,
                &m.create.src_mbr,
            )
            .ok()
            .flatten()
            .unwrap_or_default(),
            None => Vec::new(),
        };
        let stmts = statements(&m, &lines);
        let view = if m.debug_code.is_none() {
            "*NONE".to_string()
        } else {
            m.ir.options.dbgview.name().to_string()
        };
        modules.push(DbgModule {
            name: m.name.clone(),
            lines,
            stmts,
            view,
        });
    }
    Ok(DbgProgram {
        lib,
        name: name.to_ascii_uppercase(),
        kind,
        modules,
    })
}

/// Where each statement starts: CL by its statements, IR source by its
/// `debug.stmt` lines, otherwise one statement a line.
fn statements(m: &ModuleObject, lines: &[String]) -> Vec<(u32, u32)> {
    if m.create.compiler == "CLLE" && !lines.is_empty() {
        let (raws, _) = cl::lex::statements(&lines.join("\n"));
        return raws
            .iter()
            .filter_map(|r| Some((u32::try_from(r.line).ok()?, u32::try_from(r.n).ok()?)))
            .collect();
    }
    let marked: Vec<(u32, u32)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let n = l.trim().strip_prefix("debug.stmt")?.trim().parse().ok()?;
            Some((u32::try_from(i + 1).ok()?, n))
        })
        .collect();
    if !marked.is_empty() {
        return marked;
    }
    (1..=u32::try_from(lines.len()).unwrap_or(0))
        .map(|l| (l, l))
        .collect()
}
