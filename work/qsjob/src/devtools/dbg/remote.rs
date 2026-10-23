// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Debugging another job: `STRSRVJOB` names it to the storage service,
//! which keeps a mailbox each way. The servicing job posts orders
//! (`STRDBG`, the breakpoints, resume, `EVAL`); the serviced job takes
//! them when it calls a program, at each break poll while one runs and
//! while it is stopped, and posts its stops and answers back.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use cmd::{Resolved, Value};
use dbg::session::{StopAt, StopReason};
use mi::Job;
use mi::debug::{Resume, Stop};
use stg::Client;
use stg::proto::{DbgEvent, DbgOrder};

use super::State;
use crate::command::Fail;
use crate::console::host::Host;

thread_local! {
    /// This job is debugged from another one.
    static SERVED: Cell<bool> = const { Cell::new(false) };
    /// Events the servicing side took but has not handled.
    static PENDING: RefCell<VecDeque<DbgEvent>> = const { RefCell::new(VecDeque::new()) };
}

/// How often a stopped job looks for orders.
const POLL: Duration = Duration::from_millis(200);
/// How long an `EVAL` waits for the serviced job's answer.
const ANSWER: Duration = Duration::from_secs(10);

/// The job this one services, if any.
pub fn serving(host: &Host) -> Option<String> {
    Some(host.link.borrow_mut().with(Client::srvjob_serving)).filter(|s| !s.is_empty())
}

/// An order for the serviced job.
pub fn order(host: &Host, o: DbgOrder) -> Result<(), Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.srvjob_order(o))
        .map_err(|e: String| fail(&e))
}

/// A service error as a failure: its message identifier first.
fn fail(e: &str) -> Fail {
    match e.split_once(' ') {
        Some((id, text)) if id.len() == 7 => Fail::new(id, text.to_string()),
        _ => Fail::new("CPF1999", e.to_string()),
    }
}

/// `STRSRVJOB JOB(number/user/name)`.
pub fn strsrvjob(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parts: Vec<String> = match r.get("JOB").and_then(|b| b.value.as_ref()) {
        Some(Value::Qual(p)) => p.iter().map(|v| v.source().trim().to_ascii_uppercase()).collect(),
        Some(v) => v
            .source()
            .split('/')
            .rev()
            .map(|s| s.trim().to_ascii_uppercase())
            .collect(),
        None => Vec::new(),
    };
    // The qualifiers come name first; the service takes number/user/name.
    let job: Vec<&str> = parts.iter().rev().map(String::as_str).filter(|s| !s.is_empty()).collect();
    if job.is_empty() {
        return Err(Fail::new("CPD0071", "A job name is needed."));
    }
    let q = host
        .link
        .borrow_mut()
        .with(|l| l.srvjob_start(job.join("/")))
        .map_err(|e: String| fail(&e))?;
    Ok(format!("Job {q} is now serviced by this job.\n"))
}

/// `ENDSRVJOB`.
pub fn endsrvjob(job: &mut Job, host: &Host) -> Result<String, Fail> {
    let _ = job;
    let q = host
        .link
        .borrow_mut()
        .with(Client::srvjob_end)
        .map_err(|e: String| fail(&e))?;
    super::forget();
    PENDING.with(|p| p.borrow_mut().clear());
    Ok(format!("Job {q} is no longer serviced.\n"))
}

/// Take the serviced job's news into the pending queue; whether any
/// waits.
pub fn has_events(host: &Host) -> bool {
    let new = host.link.borrow_mut().with(Client::srvjob_events);
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        p.extend(new);
        !p.is_empty()
    })
}

/// Handle what the serviced job sent: a stop shows (unless its
/// breakpoint's condition does not hold); the message for the display.
pub fn take_stop(job: &mut Job, host: &Host, st: &mut State) -> Option<String> {
    let _ = job;
    loop {
        let e = PENDING.with(|p| p.borrow_mut().pop_front())?;
        match e {
            DbgEvent::Stopped {
                program,
                procedure,
                stmt,
            } => {
                let bkpt = st
                    .session
                    .bkpts
                    .list
                    .iter()
                    .find(|b| b.program == program && b.stmt == stmt)
                    .cloned();
                if let Some(c) = bkpt
                    .as_ref()
                    .and_then(|b| b.when.as_deref())
                    .and_then(dbg::cond::parse)
                {
                    let shown = eval(host, &c.var).unwrap_or_default();
                    if !c.holds(&shown) {
                        let _ = order(host, DbgOrder::Resume { step: false, into: false });
                        continue;
                    }
                }
                if !st.session.modules.iter().any(|m| m.program == program) {
                    let (lib, name) = program.split_once('/').unwrap_or(("*LIBL", &program));
                    if let Ok(m) = super::load(host, lib, name) {
                        super::add_program(st, m);
                    }
                }
                st.session.stop(StopAt {
                    program: program.clone(),
                    procedure,
                    stmt,
                    reason: if bkpt.is_some() {
                        StopReason::Breakpoint
                    } else {
                        StopReason::Step
                    },
                });
                let line = st
                    .session
                    .module()
                    .and_then(|m| m.map.line_of(stmt))
                    .map_or_else(|| format!("statement {stmt}"), |l| format!("line {l}"));
                return Some(format!("The serviced job stopped at {line} of {program}."));
            }
            DbgEvent::Running => st.session.resumed(),
            DbgEvent::Answer { text } => return Some(text),
        }
    }
}

/// `EVAL` in the serviced job: its answer, waited for.
pub fn eval(host: &Host, expr: &str) -> Result<String, String> {
    order(host, DbgOrder::Eval { expr: expr.into() }).map_err(|f| f.text)?;
    let until = Instant::now() + ANSWER;
    while Instant::now() < until {
        let new = host.link.borrow_mut().with(Client::srvjob_events);
        let mut answer = None;
        for e in new {
            match e {
                DbgEvent::Answer { text } if answer.is_none() => answer = Some(text),
                other => PENDING.with(|p| p.borrow_mut().push_back(other)),
            }
        }
        if let Some(a) = answer {
            return a.strip_prefix('!').map_or(Ok(a.clone()), |e| Err(e.to_string()));
        }
        std::thread::sleep(POLL);
    }
    Err("The serviced job did not answer.".into())
}

/// Whether this job is debugged from another one.
pub fn is_served() -> bool {
    SERVED.with(Cell::get)
}

/// The serviced side: take and do the orders waiting for this job.
/// Called before a program is called and at each break poll.
pub fn served(job: &mut Job, host: &Host) {
    let orders = host.link.borrow_mut().with(Client::srvjob_orders);
    for o in orders {
        let _ = apply(job, host, o);
    }
}

/// One order; `Some` when it resumes a stop.
fn apply(job: &mut Job, host: &Host, o: DbgOrder) -> Option<Resume> {
    match o {
        DbgOrder::Start { program, updprod } => {
            SERVED.with(|s| s.set(true));
            let (lib, name) = program.split_once('/').unwrap_or(("*LIBL", &program));
            let text = match job.strdbg(name, lib) {
                Ok(k) => {
                    if let Some(d) = job.debug.as_mut() {
                        d.updprod = updprod;
                    }
                    format!("{k} is under debug in the serviced job.")
                }
                Err(e) => format!("!{e}"),
            };
            let _ = host
                .link
                .borrow_mut()
                .with(|l| l.srvjob_event(DbgEvent::Answer { text }));
            None
        }
        DbgOrder::End => {
            SERVED.with(|s| s.set(false));
            job.enddbg();
            Some(Resume::Go)
        }
        DbgOrder::Breakpoints { list } => {
            SERVED.with(|s| s.set(true));
            job.debug
                .get_or_insert_with(mi::debug::Debugger::default)
                .breakpoints = list;
            None
        }
        DbgOrder::Resume { step, into } => {
            let r = match (step, into) {
                (false, _) => Resume::Go,
                (true, false) => Resume::StepOver,
                (true, true) => Resume::StepInto,
            };
            // Outside a stop a step waits for the next statement.
            if step && let Some(d) = job.debug.as_mut() {
                d.step = mi::debug::Step::Into;
            }
            Some(r)
        }
        DbgOrder::Eval { expr } => {
            let text = match job.eval(&expr) {
                Ok(v) => v,
                Err(e) => format!("!{e}"),
            };
            let _ = host
                .link
                .borrow_mut()
                .with(|l| l.srvjob_event(DbgEvent::Answer { text }));
            None
        }
    }
}

/// The serviced job stopped: tell the servicing job and do its orders
/// until one resumes.
pub fn stopped(job: &mut Job, at: &Stop, host: &Host) -> Resume {
    let told = host.link.borrow_mut().with(|l| {
        l.srvjob_event(DbgEvent::Stopped {
            program: at.program.clone(),
            procedure: at.procedure.clone(),
            stmt: at.stmt,
        })
    });
    if told.is_err() {
        // Nobody services this job any more.
        SERVED.with(|s| s.set(false));
        return Resume::Go;
    }
    loop {
        let orders = host.link.borrow_mut().with(Client::srvjob_orders);
        for o in orders {
            if let Some(r) = apply(job, host, o) {
                let _ = host
                    .link
                    .borrow_mut()
                    .with(|l| l.srvjob_event(DbgEvent::Running));
                if let Some(d) = job.debug.as_mut() {
                    d.step = mi::debug::Step::None;
                }
                return r;
            }
        }
        std::thread::sleep(POLL);
    }
}
