// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's debugger (version 9, draft), under the real
//! system's rules and messages:
//!
//! - `HLINV/ASSETS`, one module `ASSETS` with `*STMT` and `*SOURCE`
//!   views, [`ASSETS_SOURCE`] its 14 lines, statements on lines 9, 10,
//!   12, 13 and 14; its variables `COUNT` (integer), `NAME` (`CHAR(10)`)
//!   and the structure `ASSET` with `ASSET.ID` and `ASSET.TAG`, which
//!   change as its statements run;
//! - `HLINV/NODEBUG`, compiled `DBGVIEW(*NONE)`: `AddProgram` answers
//!   `Error` with [`NO_DEBUG_DATA`]; any other program is `NotFound`
//!   (`CPF9801`);
//! - `CALL PGM(HLINV/ASSETS)` through `RunCommand` runs it in the
//!   connection job, stopping where an `Own` session says; a `Cancel`
//!   while stopped ends the command (`CPF1907`);
//! - other jobs: [`System::interactive_job`] makes one (a display
//!   session's), [`System::call_in`] calls `ASSETS` in it on a thread of
//!   its own, as its user at the display would; a stop holds that
//!   thread until the session resumes it. [`System::end_job`] ends one.
//!
//! Conditions are `VAR = value` and `VAR <> value`; anything else is
//! `Error` with `CPF9C59`.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use qap_proto::Status;
use qap_proto::ops::debug::{
    AddProgram, BreakpointSet, BreakpointsSet, DebugModule, DebugTarget, Eval, Evaluated, Frame,
    Frames, ProgramAdded, RemoveServiceEntry, Resume, ResumeHow, SetBreakpoints, SetServiceEntry,
    Start, Started, StopReason, Stopped, Variable, VariableList, Variables, View,
};
use qap_proto::types::{JobId, ObjectName};

use super::host::MemThread;
use super::{System, lock};
use crate::debug::{
    DebugHost, JOB_ENDED, NO_ENTRY, NO_SESSION, News, OTHER_USER, SESSION_ACTIVE,
};
use crate::host::{Refusal, ThreadHost};

/// `ASSETS`'s source.
pub const ASSETS_SOURCE: [&str; 14] = [
    "**FREE",
    "// ASSETS: count the assets in service.",
    "dcl-s count int(10) inz(0);",
    "dcl-s name char(10) inz('DESK');",
    "dcl-ds asset qualified;",
    "  id int(10);",
    "  tag char(8);",
    "end-ds;",
    "count = count + 1;",
    "asset.id = count;",
    "// the tag",
    "asset.tag = 'A' + %char(count);",
    "name = 'CHAIR';",
    "*inlr = *on;",
];

/// The lines of `ASSETS` that start statements; each is its statement
/// number, as the RPG compiler numbers them.
pub const STATEMENTS: [u32; 5] = [9, 10, 12, 13, 14];

/// What `AddProgram` of a program compiled `DBGVIEW(*NONE)` answers.
pub const NO_DEBUG_DATA: &str = "CPF9C02";

/// `HLINV/ASSETS`.
pub const ASSETS: &str = "HLINV/ASSETS";

/// The variables of `ASSETS` before its statement `at` (an index into
/// [`STATEMENTS`]; past the end, after the last).
fn values(at: usize) -> Vec<(&'static str, &'static str, String, bool)> {
    let count = i32::from(at >= 1);
    let id = i32::from(at >= 2);
    let tag = if at >= 3 { "'A1      '" } else { "'        '" };
    let name = if at >= 4 {
        "'CHAIR     '"
    } else {
        "'DESK      '"
    };
    vec![
        ("COUNT", "int(10)", count.to_string(), false),
        ("NAME", "char(10)", name.to_string(), false),
        ("ASSET", "ds(12)", String::new(), true),
        ("ASSET.ID", "int(10)", id.to_string(), false),
        ("ASSET.TAG", "char(8)", tag.to_string(), false),
    ]
}

/// A breakpoint: its statement and condition.
#[derive(Clone, Debug)]
struct Bkpt {
    stmt: u32,
    cond: Option<(String, bool, String)>,
}

/// Where a session's job is stopped.
#[derive(Clone, Debug)]
struct At {
    index: usize,
}

/// One connection's debug session.
struct Session {
    /// `Job`'s job.
    target: Option<JobId>,
    /// A job a service entry point attached.
    attached: Option<JobId>,
    programs: Vec<String>,
    bkpts: Vec<Bkpt>,
    stopped: Option<At>,
    resume: Option<ResumeHow>,
    news: VecDeque<News>,
}

/// A service entry point.
struct Entry {
    owner: u32,
    program: String,
    user: String,
}

/// A job other than the connections'.
struct Other {
    id: JobId,
    ended: bool,
}

#[derive(Default)]
struct State {
    sessions: HashMap<u32, Session>,
    numbers: u32,
    entries: Vec<Entry>,
    jobs: Vec<Other>,
}

/// The debugger's state.
#[derive(Default)]
pub struct Debugs {
    state: Mutex<State>,
    cv: Condvar,
    jobs: AtomicU32,
}

fn key(o: &ObjectName) -> String {
    let lib = match o.library.trim().to_ascii_uppercase() {
        l if l.is_empty() || l == "*LIBL" || l == "*CURLIB" => "HLINV".to_string(),
        l => l,
    };
    format!("{lib}/{}", o.name.trim().to_ascii_uppercase())
}

fn frame(index: usize) -> Frame {
    let line = STATEMENTS.get(index).copied().unwrap_or(14);
    Frame {
        program: ObjectName {
            library: "HLINV".into(),
            name: "ASSETS".into(),
        },
        module: "ASSETS".into(),
        procedure: "ASSETS".into(),
        stmt: line,
        line,
        view: View::Source,
    }
}

/// A condition as the session keeps it: variable, equal, value.
fn condition(text: &str) -> Option<(String, bool, String)> {
    let (l, r, eq) = if let Some((l, r)) = text.split_once("<>") {
        (l, r, false)
    } else {
        let (l, r) = text.split_once('=')?;
        (l, r, true)
    };
    let (l, r) = (l.trim(), r.trim());
    if l.is_empty() || r.is_empty() {
        return None;
    }
    let r = r.trim_matches('\'').trim();
    Some((l.to_ascii_uppercase(), eq, r.to_string()))
}

fn holds(c: &(String, bool, String), at: usize) -> bool {
    let v = values(at)
        .into_iter()
        .find(|(n, ..)| *n == c.0)
        .map(|(_, _, v, _)| v.trim_matches('\'').trim().to_string())
        .unwrap_or_default();
    (v == c.2) == c.1
}

impl Debugs {
    fn wait<'a>(&self, g: std::sync::MutexGuard<'a, State>) -> std::sync::MutexGuard<'a, State> {
        self.cv
            .wait_timeout(g, Duration::from_millis(10))
            .unwrap_or_else(PoisonError::into_inner)
            .0
    }

    /// Run `ASSETS` in `job` for session `owner`'s debugging, stopping
    /// where it says; `from_entry` when a service entry point attached
    /// the job. Whether it ran to its end (not cancelled).
    fn run(&self, owner: u32, job: &JobId, from_entry: bool, cancelled: &dyn Fn() -> bool) -> bool {
        let mut step: Option<ResumeHow> = None;
        for (index, stmt) in STATEMENTS.iter().enumerate() {
            let mut g = lock(&self.state);
            let Some(s) = g.sessions.get_mut(&owner) else {
                return true;
            };
            if !from_entry && !s.programs.iter().any(|p| p == ASSETS) {
                return true;
            }
            let bkpt = s
                .bkpts
                .iter()
                .find(|b| b.stmt == *stmt && b.cond.as_ref().is_none_or(|c| holds(c, index)))
                .is_some();
            let reason = if bkpt {
                StopReason::Breakpoint
            } else if matches!(step, Some(ResumeHow::StepOver | ResumeHow::StepInto)) {
                StopReason::Step
            } else if from_entry && index == 0 {
                StopReason::ServiceEntry
            } else {
                continue;
            };
            s.stopped = Some(At { index });
            s.resume = None;
            s.news.push_back(News::Stopped(Stopped {
                job: job.clone(),
                reason,
                frame: frame(index),
            }));
            self.cv.notify_all();
            loop {
                if cancelled() {
                    if let Some(s) = g.sessions.get_mut(&owner) {
                        s.stopped = None;
                    }
                    return false;
                }
                let Some(s) = g.sessions.get_mut(&owner) else {
                    return true;
                };
                if let Some(how) = s.resume.take() {
                    s.stopped = None;
                    step = Some(how);
                    break;
                }
                g = self.wait(g);
            }
        }
        true
    }
}

impl System {
    /// A display session's job running as `user`, for `call_in`.
    pub fn interactive_job(&self, user: &str) -> JobId {
        let n = 700 + self.debug.jobs.fetch_add(1, Ordering::SeqCst);
        let id = JobId {
            name: format!("QPADEV{n:04}"),
            user: user.to_ascii_uppercase(),
            number: format!("{n:06}"),
        };
        lock(&self.debug.state).jobs.push(Other {
            id: id.clone(),
            ended: false,
        });
        id
    }

    /// `CALL HLINV/ASSETS` in `job`, as its user types it: a service
    /// entry point for `ASSETS` and the job's user attaches it to the
    /// session that set it, and a session debugging the job stops it
    /// where it says. The thread ends when the program does.
    pub fn call_in(self: &Arc<Self>, job: &JobId) -> JoinHandle<()> {
        let (sys, job) = (Arc::clone(self), job.clone());
        std::thread::spawn(move || {
            let d = &sys.debug;
            let (owner, from_entry) = {
                let mut g = lock(&d.state);
                let by_target = g
                    .sessions
                    .iter()
                    .find(|(_, s)| s.target.as_ref() == Some(&job) || s.attached.as_ref() == Some(&job))
                    .map(|(o, _)| *o);
                if let Some(o) = by_target {
                    (Some(o), false)
                } else {
                    let hit = g
                        .entries
                        .iter()
                        .find(|e| e.program == ASSETS && e.user == job.user)
                        .map(|e| e.owner);
                    match hit.and_then(|o| g.sessions.get_mut(&o).map(|s| (o, s))) {
                        Some((o, s)) if s.target.is_none() && s.attached.is_none() => {
                            s.attached = Some(job.clone());
                            // The entry puts its program under debug.
                            if !s.programs.iter().any(|p| p == ASSETS) {
                                s.programs.push(ASSETS.into());
                            }
                            (Some(o), true)
                        }
                        _ => (None, false),
                    }
                }
            };
            if let Some(o) = owner {
                d.run(o, &job, from_entry, &|| false);
            }
        })
    }

    /// The job ends: a session debugging it ends with `DebugEnded`.
    pub fn end_job(&self, job: &JobId) {
        let mut g = lock(&self.debug.state);
        for o in &mut g.jobs {
            if o.id == *job {
                o.ended = true;
            }
        }
        self.debug.cv.notify_all();
    }

    /// `CALL PGM(...)` in connection job `job`: `ASSETS` runs with an
    /// `Own` session's stops; whether it ended without a cancel.
    pub(super) fn call_own(&self, job: u32, pgm: &str, cancelled: &dyn Fn() -> bool) -> bool {
        let name = pgm.trim().trim_matches('\'').to_ascii_uppercase();
        if !(name == ASSETS || name == "ASSETS" || name == "*LIBL/ASSETS") {
            return true;
        }
        let me = JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: format!("{job:06}"),
        };
        let own = lock(&self.debug.state)
            .sessions
            .get(&job)
            .is_some_and(|s| s.target.is_none());
        if !own {
            return true;
        }
        self.debug.run(job, &me, false, cancelled)
    }
}

impl MemThread {
    fn refused(&mut self, status: Status, id: &str, data: &[&str]) -> Refusal {
        Refusal {
            status,
            message: self.message(id, data),
        }
    }

    fn special_service(&self) -> bool {
        let me = lock(&self.sys.running_as).get(&self.job).cloned().unwrap_or_default();
        self.sys.get(&me).special.iter().any(|s| s == "*SERVICE")
    }

    fn user(&self) -> String {
        lock(&self.sys.running_as).get(&self.job).cloned().unwrap_or_default()
    }

    /// The session, or `Error` with `TCP1A71`.
    fn with_session<T>(
        &mut self,
        f: impl FnOnce(&mut Session) -> Result<T, (Status, &'static str, Vec<String>)>,
    ) -> Result<T, Refusal> {
        let r = {
            let mut g = lock(&self.sys.debug.state);
            match g.sessions.get_mut(&self.job) {
                Some(s) => f(s),
                None => Err((Status::Error, NO_SESSION, Vec::new())),
            }
        };
        self.sys.debug.cv.notify_all();
        r.map_err(|(st, id, data)| {
            let d: Vec<&str> = data.iter().map(String::as_str).collect();
            self.refused(st, id, &d)
        })
    }
}

/// `Error` with `CPD9CCC`: `what` needs a stopped job.
fn not_stopped(what: &str) -> (Status, &'static str, Vec<String>) {
    (Status::Error, "CPD9CCC", vec![what.to_string()])
}

impl DebugHost for MemThread {
    fn start(&mut self, req: &Start) -> Result<Started, Refusal> {
        let user = self.user();
        let service = self.special_service();
        let mut g = lock(&self.sys.debug.state);
        if g.sessions.contains_key(&self.job) {
            drop(g);
            return Err(self.refused(Status::InUse, SESSION_ACTIVE, &[]));
        }
        let target = match &req.target {
            DebugTarget::Own => None,
            DebugTarget::Job { job } => {
                let name = |j: &JobId| format!("{}/{}/{}", j.number, j.user, j.name);
                let found = g.jobs.iter().find(|o| {
                    !o.ended
                        && o.id.name.eq_ignore_ascii_case(job.name.trim())
                        && (job.user.trim().is_empty() || o.id.user.eq_ignore_ascii_case(job.user.trim()))
                        && (job.number.trim().is_empty() || o.id.number == job.number.trim())
                });
                let Some(o) = found else {
                    drop(g);
                    let shown = name(job);
                    return Err(self.refused(Status::NotFound, "CPF3520", &[&shown]));
                };
                let id = o.id.clone();
                if id.user != user && !service {
                    drop(g);
                    return Err(self.refused(Status::NotAuthorized, "CPF3502", &[&name(&id)]));
                }
                if g
                    .sessions
                    .values()
                    .any(|s| s.target.as_ref() == Some(&id) || s.attached.as_ref() == Some(&id))
                {
                    drop(g);
                    return Err(self.refused(Status::InUse, "CPF3503", &[&name(&id)]));
                }
                Some(id)
            }
        };
        g.numbers += 1;
        let number = g.numbers;
        g.sessions.insert(
            self.job,
            Session {
                target,
                attached: None,
                programs: Vec::new(),
                bkpts: Vec::new(),
                stopped: None,
                resume: None,
                news: VecDeque::new(),
            },
        );
        Ok(Started { session: number })
    }

    fn end(&mut self) {
        let mut g = lock(&self.sys.debug.state);
        g.sessions.remove(&self.job);
        let me = self.job;
        g.entries.retain(|e| e.owner != me);
        self.sys.debug.cv.notify_all();
    }

    fn active(&mut self) -> bool {
        lock(&self.sys.debug.state).sessions.contains_key(&self.job)
    }

    fn add_program(&mut self, req: &AddProgram) -> Result<ProgramAdded, Refusal> {
        let k = key(&req.program);
        let ty = req.r#type.trim().to_ascii_uppercase();
        if !(ty.is_empty() || ty == "*PGM" || ty == "*SRVPGM") {
            return Err(self.refused(Status::InvalidRequest, "CPD0084", &[&ty]));
        }
        if k == "HLINV/NODEBUG" {
            return Err(self.refused(Status::Error, NO_DEBUG_DATA, &[]));
        }
        if k != ASSETS {
            let (lib, name) = k.split_once('/').unwrap_or(("", &k));
            return Err(self.refused(Status::NotFound, "CPF9801", &[name, lib]));
        }
        self.with_session(|s| {
            if !s.programs.contains(&k) {
                s.programs.push(k.clone());
            }
            Ok(ProgramAdded {
                modules: vec![DebugModule {
                    name: "ASSETS".into(),
                    views: vec![View::Statement, View::Source],
                }],
            })
        })
    }

    fn set_breakpoints(&mut self, req: &SetBreakpoints) -> Result<BreakpointsSet, Refusal> {
        let k = key(&req.program);
        let module = req.module.trim().to_ascii_uppercase();
        self.with_session(|s| {
            if k != ASSETS || !s.programs.contains(&k) {
                let (lib, name) = k.split_once('/').unwrap_or(("", &k));
                return Err((Status::NotFound, "CPF9801", vec![name.into(), lib.into()]));
            }
            if module != "ASSETS" && !module.is_empty() {
                return Err((Status::NotFound, "CPF9801", vec![module, "HLINV".into()]));
            }
            let mut list = Vec::new();
            let mut set = Vec::new();
            for l in &req.lines {
                let Some(stmt) = STATEMENTS.iter().copied().find(|s| *s >= l.line) else {
                    return Err((Status::Error, "CPD9CCB", vec![l.line.to_string()]));
                };
                let cond = match &l.condition {
                    Some(c) => Some(
                        condition(c).ok_or((Status::Error, "CPF9C59", vec![c.clone()]))?,
                    ),
                    None => None,
                };
                list.push(Bkpt { stmt, cond });
                set.push(BreakpointSet { line: stmt, stmt });
            }
            s.bkpts = list;
            Ok(BreakpointsSet { set })
        })
    }

    fn resume(&mut self, req: &Resume) -> Result<(), Refusal> {
        self.with_session(|s| {
            if s.stopped.is_none() {
                return Err(not_stopped("Resume"));
            }
            s.resume = Some(req.how);
            Ok(())
        })
    }

    fn stack(&mut self) -> Result<Frames, Refusal> {
        self.with_session(|s| match &s.stopped {
            Some(at) => Ok(Frames {
                frames: vec![frame(at.index)],
            }),
            None => Err(not_stopped("Stack")),
        })
    }

    fn variables(&mut self, req: &Variables) -> Result<VariableList, Refusal> {
        self.with_session(|s| {
            let Some(at) = &s.stopped else {
                return Err(not_stopped("Variables"));
            };
            if req.frame != 0 {
                return Err((Status::InvalidRequest, "TCP1A77", vec![req.frame.to_string()]));
            }
            let parent = req.parent.as_ref().map(|p| p.trim().to_ascii_uppercase());
            let variables = values(at.index)
                .into_iter()
                .filter(|(n, ..)| match &parent {
                    Some(p) => n.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with('.')),
                    None => !n.contains('.'),
                })
                .map(|(n, t, v, e)| Variable {
                    name: n.into(),
                    r#type: t.into(),
                    value: v,
                    expandable: e,
                })
                .collect();
            Ok(VariableList { variables })
        })
    }

    fn eval(&mut self, req: &Eval) -> Result<Evaluated, Refusal> {
        self.with_session(|s| {
            let Some(at) = &s.stopped else {
                return Err(not_stopped("Eval"));
            };
            if req.frame != 0 {
                return Err((Status::InvalidRequest, "TCP1A77", vec![req.frame.to_string()]));
            }
            let (name, mode) = req
                .expression
                .split_once(':')
                .map_or((req.expression.trim(), ""), |(n, m)| (n.trim(), m.trim()));
            let v = values(at.index)
                .into_iter()
                .find(|(n, ..)| n.eq_ignore_ascii_case(name))
                .ok_or((Status::Error, "CPF7E12", vec![name.to_string()]))?;
            let value = if mode.eq_ignore_ascii_case("x") {
                v.2.bytes().fold(String::new(), |mut out, b| {
                    let _ = write!(out, "{b:02X}");
                    out
                })
            } else {
                v.2
            };
            Ok(Evaluated { value })
        })
    }

    fn set_service_entry(&mut self, req: &SetServiceEntry) -> Result<(), Refusal> {
        let k = key(&req.program);
        let me = self.user();
        let user = match req.user.trim().to_ascii_uppercase() {
            u if u.is_empty() || u == "*CURRENT" => me.clone(),
            u => u,
        };
        if user != me && !self.special_service() {
            return Err(self.refused(Status::NotAuthorized, OTHER_USER, &[&user]));
        }
        if k != ASSETS {
            let (lib, name) = k.split_once('/').unwrap_or(("", &k));
            return Err(self.refused(Status::NotFound, "CPF9801", &[name, lib]));
        }
        let owner = self.job;
        self.with_session(|_| Ok(()))?;
        let mut g = lock(&self.sys.debug.state);
        g.entries.retain(|e| !(e.owner == owner && e.program == k));
        g.entries.push(Entry {
            owner,
            program: k,
            user,
        });
        Ok(())
    }

    fn remove_service_entry(&mut self, req: &RemoveServiceEntry) -> Result<(), Refusal> {
        let k = key(&req.program);
        let owner = self.job;
        self.with_session(|_| Ok(()))?;
        let gone = {
            let mut g = lock(&self.sys.debug.state);
            let before = g.entries.len();
            g.entries.retain(|e| !(e.owner == owner && e.program == k));
            before != g.entries.len()
        };
        if gone {
            Ok(())
        } else {
            Err(self.refused(Status::NotFound, NO_ENTRY, &[&k]))
        }
    }

    fn news(&mut self) -> Vec<News> {
        let mut g = lock(&self.sys.debug.state);
        let ended: Vec<JobId> = g.jobs.iter().filter(|o| o.ended).map(|o| o.id.clone()).collect();
        let Some(s) = g.sessions.get_mut(&self.job) else {
            return Vec::new();
        };
        let mut out: Vec<News> = s.news.drain(..).collect();
        if let Some(t) = s.target.clone().filter(|t| ended.contains(t)) {
            g.sessions.remove(&self.job);
            drop(g);
            let shown = format!("{}/{}/{}", t.number, t.user, t.name);
            out.push(News::Ended(Some(self.message(JOB_ENDED, &[&shown]))));
        }
        out
    }
}
