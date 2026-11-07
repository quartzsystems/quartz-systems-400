// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection job's own thread under an `Own` session: the programs
//! added go under debug and the breakpoints reach the core at the next
//! call and break poll ([`apply`]); a stop is told to the client and
//! the thread waits, answering the secondary threads' orders, until a
//! `Resume`, the session's end, a `Cancel` of the command or the job's
//! end ([`stop`]).

use std::cell::RefCell;
use std::time::Duration;

use mi::Job;
use mi::debug::{Resume, Stop};
use qap_proto::ops::debug::{ResumeHow, StopReason, Stopped};
use qap_proto::types::JobId;
use qapd::debug::News;
use wm::ctl::ToJob;

use super::{Order, SESSION, WAKE, lock};
use crate::console::host::Host;

/// How long the stopped thread waits between looks at its job's end.
const LOOK: Duration = Duration::from_millis(200);

thread_local! {
    /// The programs this thread put under debug for the session.
    static APPLIED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The session's programs and breakpoints, as the core takes them.
type Wanted = (Vec<String>, Vec<(String, u32)>);

/// Bring the core up to the session: its programs under debug, its
/// breakpoints; after the session's end, debug ended. Called before a
/// program is called and at each break poll.
pub fn apply(job: &mut Job) {
    let wanted: Option<Wanted> = {
        let g = lock(&SESSION);
        g.as_ref().filter(|s| s.target.is_none()).map(|s| {
            (
                s.programs.iter().map(|p| p.key.clone()).collect(),
                s.core_breakpoints(),
            )
        })
    };
    let Some((programs, bkpts)) = wanted else {
        let had = APPLIED.with(|a| !a.borrow().is_empty());
        if had && job.act.frames.is_empty() {
            job.enddbg();
            APPLIED.with(|a| a.borrow_mut().clear());
        }
        return;
    };
    for key in programs {
        if APPLIED.with(|a| a.borrow().contains(&key)) {
            continue;
        }
        let (lib, name) = key.split_once('/').unwrap_or(("*LIBL", &key));
        if let Ok(found) = job.strdbg(name, lib) {
            APPLIED.with(|a| {
                let mut a = a.borrow_mut();
                a.push(found);
                a.push(key);
            });
        }
    }
    if !APPLIED.with(|a| a.borrow().is_empty()) {
        job.debug
            .get_or_insert_with(mi::debug::Debugger::default)
            .breakpoints = bkpts;
    }
}

/// The core stopped the connection job: `None` when no `Own` session
/// is active or the program is not one of its.
// BLOCKED(rel): stops are breakpoints, steps and entry points only; the
// core has no stop for an unhandled exception (`StopReason::Exception`)
// or a step past a program's end (`ProgramEnd`), so neither is sent.
pub fn stop(job: &mut Job, at: &Stop, host: &Host) -> Option<Resume> {
    let me = JobId {
        name: host.start.name.clone(),
        user: host.start.user.clone(),
        number: format!("{:06}", host.start.number),
    };
    // A condition is evaluated before anything is told.
    // A program this thread put under debug never stops elsewhere: the
    // console's session would wait for a display the job has not got.
    let ours = APPLIED.with(|a| a.borrow().contains(&at.program));
    let (reason, frame) = {
        let mut g = lock(&SESSION);
        let Some(s) = g
            .as_mut()
            .filter(|s| s.target.is_none() && s.program(&at.program).is_some())
        else {
            return ours.then_some(Resume::Go);
        };
        let bkpt = s.bkpt_at(&at.program, at.stmt).cloned();
        let holds = bkpt
            .as_ref()
            .and_then(|b| b.cond.as_deref())
            .and_then(dbg::cond::parse)
            .is_none_or(|c| c.holds(&job.eval(&c.var).unwrap_or_default()));
        let reason = if bkpt.is_some() && holds {
            StopReason::Breakpoint
        } else if s.stepping {
            StopReason::Step
        } else {
            // A breakpoint whose condition does not hold.
            return Some(Resume::Go);
        };
        let top = job.dbg_frames().into_iter().next().unwrap_or_default();
        (reason, s.frame(&top))
    };
    {
        let mut g = lock(&SESSION);
        let s = g.as_mut()?;
        s.own_stopped = true;
        s.news.push_back(News::Stopped(Stopped {
            job: me,
            reason,
            frame,
        }));
    }
    let how = serve(job, host);
    if let Some(s) = lock(&SESSION).as_mut() {
        s.own_stopped = false;
        s.stepping = !matches!(how, ResumeHow::Go);
    }
    Some(match how {
        ResumeHow::Go => Resume::Go,
        ResumeHow::StepOver => Resume::StepOver,
        ResumeHow::StepInto => Resume::StepInto,
        ResumeHow::StepOut => Resume::StepOut,
    })
}

/// Answer the orders until one resumes; `Go` when the session, the
/// command or the job ends meanwhile.
fn serve(job: &mut Job, host: &Host) -> ResumeHow {
    loop {
        if crate::sysdsp::keys::outside_end_asked() || told_to_end(host) {
            return ResumeHow::Go;
        }
        let order = {
            let mut g = lock(&SESSION);
            let Some(s) = g.as_mut() else {
                return ResumeHow::Go;
            };
            // Breakpoints changed while stopped reach the core now.
            if let Some(d) = job.debug.as_mut() {
                d.breakpoints = s.core_breakpoints();
            }
            if let Some(o) = s.orders.pop_front() {
                Some(o)
            } else {
                let (mut g, _) = WAKE
                    .wait_timeout(g, LOOK)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                g.as_mut().and_then(|s| s.orders.pop_front())
            }
        };
        match order {
            Some(Order::Resume(how)) => return how,
            Some(Order::Frames(tx)) => {
                let _ = tx.send(job.dbg_frames());
            }
            Some(Order::Vars(frame, parent, tx)) => {
                let _ = tx.send(job.dbg_variables(frame as usize, parent.as_deref()));
            }
            Some(Order::Eval(frame, expr, tx)) => {
                let _ = tx.send(job.eval_in(frame as usize, &expr));
            }
            None => {}
        }
    }
}

/// Whether the job was told to end: the message stays for the
/// connection to see after the command.
fn told_to_end(host: &Host) -> bool {
    while let Ok(m) = host.inbox.try_recv() {
        host.pending.borrow_mut().push_back(m);
    }
    host.pending
        .borrow()
        .iter()
        .any(|m| matches!(m, ToJob::End { .. }))
}
