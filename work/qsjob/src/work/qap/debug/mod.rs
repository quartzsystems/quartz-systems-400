// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Debug` service in the connection job (QAP version 9, draft): the
//! connection's one debug session, kept for the whole process since a
//! connection job serves one connection. Its secondary threads answer
//! the requests ([`thread`]); the job's own thread, where `RunCommand`
//! calls programs, stops in them ([`own`]); another job is reached as
//! `STRSRVJOB` reaches it, through the storage service's mailboxes
//! (`stgsvc/dbg.rs`), and a service entry point attaches one there.
//! What the core debugger finds (`mi::debug`) is reported over QAP
//! instead of on a display.

pub mod own;
pub mod thread;

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use mi::debug::{DbgFrame, DbgValue};
use qap_proto::ops::debug::{Frame, ResumeHow, View};
use qap_proto::types::{JobId, ObjectName};
use qapd::debug::News;
use stg::proto::{DbgEvent, DbgModule};

/// A program under debug, as the storage service described it.
#[derive(Clone, Debug)]
pub(super) struct Program {
    /// `LIB/NAME`.
    pub key: String,
    /// Its modules.
    pub modules: Vec<DbgModule>,
}

/// A breakpoint: where it is, and its condition.
#[derive(Clone, Debug)]
pub(super) struct Bkpt {
    /// `LIB/NAME`.
    pub program: String,
    /// The module.
    pub module: String,
    /// The statement.
    pub stmt: u32,
    /// `variable op value`.
    pub cond: Option<String>,
}

/// What the job's own thread is asked while it is stopped.
pub(super) enum Order {
    /// The call stack.
    Frames(Sender<Vec<DbgFrame>>),
    /// A stack entry's variables.
    Vars(u32, Option<String>, Sender<Result<Vec<DbgValue>, String>>),
    /// `EVAL` in a stack entry.
    Eval(u32, String, Sender<Result<String, String>>),
    /// Go on.
    Resume(ResumeHow),
}

/// The connection's session.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)] // independent states of one session
pub(super) struct Session {
    /// `Job`'s job, `number/user/name`.
    pub target: Option<String>,
    /// A job a service entry point attached, `number/user/name`.
    pub attached: Option<String>,
    /// The next stop of the attached job is its entry.
    pub entry_pending: bool,
    /// Programs added.
    pub programs: Vec<Program>,
    /// Breakpoints.
    pub bkpts: Vec<Bkpt>,
    /// Service entry points set, `LIB/NAME`.
    pub entries: Vec<String>,
    /// The connection job is stopped.
    pub own_stopped: bool,
    /// Orders for the stopped connection job.
    pub orders: VecDeque<Order>,
    /// The other job is stopped.
    pub remote_stopped: bool,
    /// The last resume was a step.
    pub stepping: bool,
    /// News for the client.
    pub news: VecDeque<News>,
}

impl Session {
    /// The breakpoints as the core keeps them: program and statement.
    pub fn core_breakpoints(&self) -> Vec<(String, u32)> {
        self.bkpts
            .iter()
            .map(|b| (b.program.clone(), b.stmt))
            .collect()
    }

    /// Program `key`'s description, by `LIB/NAME` or by name.
    pub fn program(&self, key: &str) -> Option<&Program> {
        let short = key.rsplit('/').next().unwrap_or(key);
        self.programs.iter().find(|p| p.key == key).or_else(|| {
            self.programs
                .iter()
                .find(|p| p.key.rsplit('/').next() == Some(short))
        })
    }

    /// The breakpoint at a statement of a program.
    pub fn bkpt_at(&self, program: &str, stmt: u32) -> Option<&Bkpt> {
        self.bkpts
            .iter()
            .find(|b| b.stmt == stmt && b.program == program)
    }

    /// A core frame as the client sees it: its line in the source view.
    pub fn frame(&self, f: &DbgFrame) -> Frame {
        let (library, name) = f
            .program
            .split_once('/')
            .map_or((String::new(), f.program.clone()), |(l, n)| {
                (l.to_string(), n.to_string())
            });
        let module = self.program(&f.program).and_then(|p| {
            p.modules
                .iter()
                .find(|m| m.name == f.module)
                .or_else(|| p.modules.first())
        });
        let (line, view) = match module {
            Some(m) if !m.lines.is_empty() => (
                m.stmts
                    .iter()
                    .find(|(_, s)| *s == f.stmt)
                    .map_or(0, |(l, _)| *l),
                View::Source,
            ),
            _ => (f.stmt, View::Statement),
        };
        Frame {
            program: ObjectName { library, name },
            module: f.module.clone(),
            procedure: f.procedure.clone(),
            stmt: f.stmt,
            line,
            view,
        }
    }
}

/// The session; `None` when none is active.
pub(super) static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// Wakes the stopped connection job.
pub(super) static WAKE: Condvar = Condvar::new();
/// The serviced job's news the threads took and have not used.
pub(super) static REMOTE: Mutex<VecDeque<DbgEvent>> = Mutex::new(VecDeque::new());

/// A lock, poison or not.
pub(super) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A job as `number/user/name`, as the client names it.
pub(super) fn job_of(q: &str) -> JobId {
    let mut p = q.split('/');
    JobId {
        number: p.next().unwrap_or_default().to_string(),
        user: p.next().unwrap_or_default().to_string(),
        name: p.next().unwrap_or_default().to_string(),
    }
}

/// The views a module's `DBGVIEW` offers.
pub(super) fn views(view: &str) -> Vec<View> {
    match view {
        "*NONE" => Vec::new(),
        "*SOURCE" | "*COPY" => vec![View::Statement, View::Source],
        "*LIST" => vec![View::Statement, View::Listing],
        "*ALL" => vec![View::Statement, View::Source, View::Listing],
        _ => vec![View::Statement],
    }
}

/// Where a breakpoint asked for `line` lands in a module: the first
/// statement on or after it that the debug code has, and its line. A
/// module without a source view takes statement numbers as lines.
pub(super) fn landing(m: &DbgModule, line: u32) -> Option<(u32, u32)> {
    let has = |s: u32| m.present.is_empty() || m.present.binary_search(&s).is_ok();
    if m.lines.is_empty() {
        return m
            .present
            .iter()
            .copied()
            .find(|s| *s >= line)
            .map(|s| (s, s));
    }
    let mut pairs = m.stmts.clone();
    pairs.sort_unstable();
    pairs.into_iter().find(|(l, s)| *l >= line && has(*s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(lines: usize, stmts: &[(u32, u32)], present: &[u32]) -> DbgModule {
        DbgModule {
            name: "M".into(),
            lines: vec![String::new(); lines],
            stmts: stmts.to_vec(),
            view: "*SOURCE".into(),
            present: present.to_vec(),
        }
    }

    #[test]
    fn breakpoints_land_on_statements_the_code_has() {
        let m = module(
            14,
            &(1..=14).map(|l| (l, l)).collect::<Vec<_>>(),
            &[9, 10, 12],
        );
        assert_eq!(landing(&m, 11), Some((12, 12)));
        assert_eq!(landing(&m, 1), Some((9, 9)));
        assert_eq!(landing(&m, 13), None);
        let stmt_only = module(0, &[], &[3, 7]);
        assert_eq!(landing(&stmt_only, 4), Some((7, 7)));
        assert_eq!(views("*ALL").len(), 3);
        assert!(views("*NONE").is_empty());
        let j = job_of("004711/BOB/QPADEV0001");
        assert_eq!((j.number.as_str(), j.user.as_str()), ("004711", "BOB"));
    }
}
