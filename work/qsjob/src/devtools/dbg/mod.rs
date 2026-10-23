// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The source debugger's displays in the job: `STRDBG` and
//! `DSPMODSRC` show Display Module Source, a stop shows it at the line,
//! and its keys and debug command line work the session of the `dbg`
//! crate; `ADDBKP`/`RMVBKP`/`DSPBKP` keep their statement breakpoints
//! beside it. `STRSRVJOB` makes the debugger act on another job: the
//! orders and stops go through the storage service (requests 380-387,
//! `remote`). A job without a display keeps the Phase 4 text session.

mod lists;
mod remote;
mod source;
mod ui;

use std::cell::RefCell;
use std::fmt::Write as _;

use cmd::{Resolved, Value};
use dbg::Session;
use dbg::bkpt::StmtMap;
use dbg::session::{Module, StopAt, StopReason};
use mi::Job;
use mi::debug::{Resume, Stop};
use stg::Client;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::panels::shows_panels;

pub use remote::served;

/// Source rows and columns of Display Module Source.
const PAGE: (usize, usize) = (dbg::screen::source::PAGE, dbg::screen::source::WIDTH);

/// What the debugger keeps between commands.
#[derive(Default)]
pub struct State {
    /// The session: modules, view, breakpoints, watches, equates.
    pub session: Session,
    /// The key page shown.
    pub keys: usize,
    /// The last debug command, for F9.
    pub last: String,
    /// Evaluate Expression's history.
    pub evals: Vec<String>,
    /// Statement breakpoints `ADDBKP` set: program and statement.
    pub stmts: Vec<(String, u32)>,
    /// Statements still to step before the next stop shows.
    pub steps_left: usize,
    /// The watched values when last seen.
    pub watched: Vec<(usize, String)>,
    /// Display output (F23): the last program output lines.
    pub output: Vec<String>,
    /// The job this one services, `number/user/name`.
    pub serving: Option<String>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// The debugger's state, taken out while a display runs.
fn take() -> State {
    STATE
        .with(|s| s.borrow_mut().take())
        .unwrap_or_else(|| State {
            session: Session::new(Vec::new(), PAGE.0, PAGE.1),
            ..State::default()
        })
}

/// Put the state back.
fn keep(st: State) {
    STATE.with(|s| *s.borrow_mut() = Some(st));
}

/// Forget it (`ENDDBG`).
fn forget() {
    STATE.with(|s| *s.borrow_mut() = None);
}

/// A program's modules as the session shows them.
pub fn load(host: &Host, lib: &str, name: &str) -> Result<Vec<Module>, String> {
    let p = host
        .link
        .borrow_mut()
        .with(|l| l.dbg_program(lib.into(), name.into()))?;
    let program = format!("{}/{}", p.lib, p.name);
    Ok(p.modules
        .into_iter()
        .map(|m| {
            let pairs = m
                .stmts
                .iter()
                .map(|(l, s)| (*l as usize, *s))
                .collect::<Vec<_>>();
            Module {
                program: program.clone(),
                module: m.name,
                kind: p.kind.clone(),
                map: StmtMap::new(pairs),
                lines: m.lines,
            }
        })
        .collect())
}

/// Add a program's modules to the session (once).
fn add_program(st: &mut State, modules: Vec<Module>) {
    for m in modules {
        if !st
            .session
            .modules
            .iter()
            .any(|o| o.program == m.program && o.module == m.module)
        {
            st.session.modules.push(m);
        }
    }
    if st.session.view.is_none() && !st.session.modules.is_empty() {
        st.session.show(0);
    }
}

/// The breakpoints to the debug core: the session's and `ADDBKP`'s,
/// in this job or the one it services.
fn sync(job: &mut Job, host: &Host, st: &State) {
    let mut list = st.session.core_breakpoints();
    list.extend(st.stmts.iter().cloned());
    if st.serving.is_some() {
        let _ = host
            .link
            .borrow_mut()
            .with(|l| l.srvjob_order(stg::proto::DbgOrder::Breakpoints { list }));
        return;
    }
    let d = job.debug.get_or_insert_with(mi::debug::Debugger::default);
    d.breakpoints = list;
}

/// `PGM(lib/name ...)` values of a list parameter.
fn programs(r: &Resolved, kwd: &str) -> Vec<(String, String)> {
    let one = |v: &Value| -> Option<(String, String)> {
        let (name, lib) = match v {
            Value::Qual(parts) => (
                parts.first().map(Value::source).unwrap_or_default(),
                parts.get(1).map(Value::source).unwrap_or_default(),
            ),
            other => {
                let s = other.source();
                match s.split_once('/') {
                    Some((l, n)) => (n.to_string(), l.to_string()),
                    None => (s, String::new()),
                }
            }
        };
        let name = name.trim().to_ascii_uppercase();
        if name.is_empty() || name.starts_with('*') {
            return None;
        }
        let lib = match lib.trim().to_ascii_uppercase() {
            l if l.is_empty() => "*LIBL".to_string(),
            l => l,
        };
        Some((lib, name))
    };
    match r.get(kwd).and_then(|b| b.value.as_ref()) {
        Some(Value::List(items)) => items.iter().filter_map(one).collect(),
        Some(v) => one(v).into_iter().collect(),
        None => Vec::new(),
    }
}

/// A parameter's single value, upper case.
fn word(r: &Resolved, kwd: &str) -> Option<String> {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(|v| v.source().trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
}

/// The debugger's commands: `None` hands a command to the text path
/// (a job without a display).
pub fn run(
    job: &mut Job,
    host: &Host,
    r: &Resolved,
    out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    let _ = out;
    match r.name.as_str() {
        "STRSRVJOB" => return Some(remote::strsrvjob(host, r)),
        "ENDSRVJOB" => return Some(remote::endsrvjob(job, host)),
        _ => {}
    }
    let display = shows_panels(host, r);
    let serving = remote::serving(host);
    if !display && serving.is_none() {
        return match r.name.as_str() {
            "DSPMODSRC" => Some(Err(Fail::new(
                "CPF9898",
                "Display Module Source needs a display; use the debug commands at this console.",
            ))),
            _ => None,
        };
    }
    let mut st = take();
    st.serving = serving;
    let res = match r.name.as_str() {
        "STRDBG" => strdbg(job, host, r, &mut st, display),
        "DSPMODSRC" => {
            if st.session.modules.is_empty() {
                Err(Fail::new("CPF1999", "No program is under debug; use STRDBG first."))
            } else {
                show_source(job, host, &mut st);
                Ok(String::new())
            }
        }
        "ENDDBG" => {
            if st.serving.is_some() {
                let _ = host
                    .link
                    .borrow_mut()
                    .with(|l| l.srvjob_order(stg::proto::DbgOrder::End));
            }
            job.enddbg();
            forget();
            return Some(Ok("Debug mode ended.\n".into()));
        }
        "ADDBKP" | "RMVBKP" => return Some(stmt_breakpoints(job, host, r, st)),
        "DSPBKP" if display => {
            lists::dspbkp(job, host, &st);
            Ok(String::new())
        }
        _ => {
            keep(st);
            return None;
        }
    };
    keep(st);
    Some(res)
}

/// `STRDBG`: the programs under debug here or in the serviced job, the
/// source display unless `DSPMODSRC(*NO)`.
fn strdbg(
    job: &mut Job,
    host: &Host,
    r: &Resolved,
    st: &mut State,
    display: bool,
) -> Result<String, Fail> {
    let mut pgms = programs(r, "PGM");
    pgms.extend(programs(r, "SRVPGM"));
    let updprod = word(r, "UPDPROD").as_deref() == Some("*YES");
    st.session.updprod = updprod;
    st.session.opmsrc = word(r, "OPMSRC").as_deref() == Some("*YES");
    let mut said = String::new();
    for (lib, name) in &pgms {
        if st.serving.is_some() {
            remote::order(
                host,
                stg::proto::DbgOrder::Start {
                    program: format!("{lib}/{name}"),
                    updprod,
                },
            )?;
        } else {
            match job.strdbg(name, lib) {
                Ok(_) => {
                    if let Some(d) = job.debug.as_mut() {
                        d.updprod = updprod;
                    }
                }
                Err(e) => return Err(Fail::new("CPF1999", e)),
            }
        }
        let modules = load(host, lib, name).map_err(|e| Fail::new("CPF1999", e))?;
        add_program(st, modules);
        let _ = writeln!(said, "{lib}/{name} is under debug.");
    }
    if pgms.is_empty() && st.session.modules.is_empty() {
        job.debug.get_or_insert_with(mi::debug::Debugger::default).updprod = updprod;
        return Ok("The job is in debug mode.\n".into());
    }
    sync(job, host, st);
    let show = word(r, "DSPMODSRC").as_deref() != Some("*NO");
    if display && show {
        show_source(job, host, st);
        return Ok(String::new());
    }
    Ok(said)
}

/// Display Module Source outside a stop; F10 leaves a step pending.
fn show_source(job: &mut Job, host: &Host, st: &mut State) {
    let mut surface = Surface::new(host.clone());
    loop {
        let stopped = st.session.stopped.is_some();
        let source::Exit::Resume(how) = source::run(job, host, &mut surface, st) else {
            return;
        };
        if st.serving.is_none() {
            if how != Resume::Go {
                // Not stopped: the next statement run stops.
                job.debug
                    .get_or_insert_with(mi::debug::Debugger::default)
                    .step = mi::debug::Step::Into;
            }
            return;
        }
        let _ = remote::order(
            host,
            stg::proto::DbgOrder::Resume {
                step: how != Resume::Go,
                into: how == Resume::StepInto,
            },
        );
        st.session.resumed();
        if !stopped {
            return;
        }
        // The serviced job runs on; the display waits for its next stop.
    }
}

/// `ADDBKP STMT(n ...) PGM()` and `RMVBKP`: statement breakpoints kept
/// beside the session's.
fn stmt_breakpoints(
    job: &mut Job,
    host: &Host,
    r: &Resolved,
    mut st: State,
) -> Result<String, Fail> {
    let program = programs(r, "PGM")
        .first()
        .map(|(_, n)| n.clone())
        .or_else(|| {
            job.debug
                .as_ref()
                .and_then(|d| d.programs.last().cloned())
        })
        .unwrap_or_default();
    if program.is_empty() {
        keep(st);
        return Err(Fail::new("CPF1999", "No program is under debug; use STRDBG first."));
    }
    let stmts: Vec<String> = match r.get("STMT").and_then(|b| b.value.as_ref()) {
        Some(Value::List(items)) => items.iter().map(|v| v.source().trim().to_string()).collect(),
        Some(v) => vec![v.source().trim().to_string()],
        None => vec!["*ALL".into()],
    };
    let mut done = 0;
    for s in &stmts {
        match (r.name.as_str(), s.parse::<u32>()) {
            ("ADDBKP", Ok(n)) => {
                if !st.stmts.contains(&(program.clone(), n)) {
                    st.stmts.push((program.clone(), n));
                }
                done += 1;
            }
            ("RMVBKP", Ok(n)) => {
                let before = st.stmts.len();
                st.stmts.retain(|(p, x)| !(p == &program && *x == n));
                done += before - st.stmts.len();
            }
            ("RMVBKP", Err(_)) => {
                let before = st.stmts.len();
                st.stmts.retain(|(p, _)| p != &program);
                done += before - st.stmts.len();
            }
            _ => {
                keep(st);
                return Err(Fail::new("CPD0084", format!("{s} is not a statement number.")));
            }
        }
    }
    sync(job, host, &st);
    let text = if r.name == "ADDBKP" {
        format!("{done} breakpoint(s) added to {program}.\n")
    } else {
        format!("{done} breakpoint(s) removed from {program}.\n")
    };
    keep(st);
    Ok(text)
}

/// The debug core stopped this job: the display at the line, or the
/// serviced job's stop handed to the job servicing it. `None` when
/// neither applies (the text session runs).
pub fn stop(job: &mut Job, at: &Stop, host: &Host) -> Option<Resume> {
    if remote::is_served() {
        return Some(remote::stopped(job, at, host));
    }
    if !host.interactive() || host.display.borrow().is_none() {
        return None;
    }
    let mut st = take();
    let resume = local_stop(job, at, host, &mut st);
    keep(st);
    Some(resume)
}

/// A stop in this job with a display.
fn local_stop(job: &mut Job, at: &Stop, host: &Host, st: &mut State) -> Resume {
    let on_bkpt = st
        .session
        .bkpts
        .list
        .iter()
        .find(|b| b.program == at.program && b.stmt == at.stmt)
        .cloned();
    let stmt_bkpt = st
        .stmts
        .iter()
        .any(|(p, s)| *s == at.stmt && (at.program == *p || at.program.ends_with(&format!("/{p}"))));
    // A condition that does not hold: on without a word.
    if let Some(b) = &on_bkpt
        && let Some(c) = b.when.as_deref().and_then(dbg::cond::parse)
    {
        let shown = job.eval(&c.var).unwrap_or_default();
        if !c.holds(&shown) {
            return Resume::Go;
        }
    }
    // Watches: a stop on the way only to look at them.
    if on_bkpt.is_none() && !stmt_bkpt && !st.session.watches.list.is_empty() {
        if let Some(n) = changed_watch(job, st) {
            return show_stop(job, host, st, at, StopReason::Watch(n));
        }
        if st.steps_left == 0 {
            return Resume::StepInto;
        }
    }
    if on_bkpt.is_none() && !stmt_bkpt && st.steps_left > 1 {
        st.steps_left -= 1;
        return Resume::StepOver;
    }
    st.steps_left = 0;
    if stmt_bkpt && on_bkpt.is_none() {
        let mut surface = Surface::new(host.clone());
        return lists::display_breakpoint(job, &mut surface, at);
    }
    let reason = if on_bkpt.is_some() {
        StopReason::Breakpoint
    } else {
        StopReason::Step
    };
    show_stop(job, host, st, at, reason)
}

/// Display Module Source at a stop; the resume the operator chose.
fn show_stop(job: &mut Job, host: &Host, st: &mut State, at: &Stop, reason: StopReason) -> Resume {
    if !st.session.modules.iter().any(|m| m.program == at.program) {
        let (lib, name) = at.program.split_once('/').unwrap_or(("*LIBL", &at.program));
        if let Ok(m) = load(host, lib, name) {
            add_program(st, m);
        }
    }
    st.session.stop(StopAt {
        program: at.program.clone(),
        procedure: at.procedure.clone(),
        stmt: at.stmt,
        reason,
    });
    remember_watches(job, st);
    let mut surface = Surface::new(host.clone());
    let exit = source::run(job, host, &mut surface, st);
    st.session.resumed();
    match exit {
        source::Exit::Resume(r) => {
            if !st.session.watches.list.is_empty() && r == Resume::Go {
                // Run on statement by statement while anything is watched.
                return Resume::StepInto;
            }
            r
        }
        source::Exit::Leave | source::Exit::Gone => Resume::Go,
    }
}

/// The values of the watched variables now.
fn remember_watches(job: &Job, st: &mut State) {
    st.watched = st
        .session
        .watches
        .list
        .iter()
        .map(|w| (w.n, job.eval(&format!("{}:x", w.expr)).unwrap_or_default()))
        .collect();
}

/// The first watch whose variable changed since last seen.
fn changed_watch(job: &Job, st: &mut State) -> Option<usize> {
    let now: Vec<(usize, String)> = st
        .session
        .watches
        .list
        .iter()
        .map(|w| (w.n, job.eval(&format!("{}:x", w.expr)).unwrap_or_default()))
        .collect();
    let hit = now
        .iter()
        .find(|(n, v)| {
            st.watched
                .iter()
                .any(|(m, old)| m == n && old != v && !old.is_empty())
        })
        .map(|(n, _)| *n);
    st.watched = now;
    hit
}

/// The job's system name for list titles.
fn sysname(host: &Host) -> String {
    crate::display::dds::system_name(host)
}
