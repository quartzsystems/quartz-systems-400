// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Debug` requests on a secondary thread of the connection job,
//! over the thread's link: programs described by the debugger's request
//! (`dbg_program`), another job reached through `STRSRVJOB`'s requests
//! and mailboxes, service entry points through the service's table. An
//! `Own` session's stopped job is asked through [`super::Order`]s.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use mi::debug::{DbgFrame, DbgValue};
use qap_proto::ops::debug::{
    AddProgram, BreakpointSet, BreakpointsSet, DebugModule, DebugTarget, Eval, Evaluated, Frames,
    ProgramAdded, RemoveServiceEntry, Resume, ResumeHow, SetBreakpoints, SetServiceEntry, Start,
    Started, StopReason, Stopped, Variable, VariableList, Variables,
};
use qap_proto::{ServerMessage, Status};
use qapd::Refusal;
use qapd::debug::{DebugHost, JOB_ENDED, NO_SESSION, News, SESSION_ACTIVE};
use stg::proto::{DbgEvent, DbgOrder};
use stg::{Client, Shared};

use super::{Bkpt, Order, Program, REMOTE, SESSION, Session, WAKE, job_of, landing, lock, views};

/// How long an answer from a stopped job may take.
const ANSWER: Duration = Duration::from_secs(10);
/// How often a thread looks for one.
const POLL: Duration = Duration::from_millis(50);
/// Sessions started in this job, numbered from 1.
static NUMBERS: AtomicU32 = AtomicU32::new(0);

/// A failure as the client gets it: status, message ID and its data.
type Fail = (Status, String, Vec<String>);

/// What `AddProgram` of a program without debug data answers: the
/// message `STRDBG` gives.
pub const NO_DEBUG_DATA: &str = "CPF9C15";

/// A secondary thread's reach of the debugger.
pub struct Thread {
    link: Shared,
}

impl Thread {
    /// Over a thread's link.
    pub fn new(link: Shared) -> Self {
        Self { link }
    }

    fn message(&self, id: &str, data: &[&str]) -> ServerMessage {
        crate::work::qap::host::message(&mut self.link.borrow_mut(), id, data)
    }

    fn refuse(&self, (status, id, data): Fail) -> Refusal {
        let d: Vec<&str> = data.iter().map(String::as_str).collect();
        Refusal {
            status,
            message: self.message(&id, &d),
        }
    }

    /// A service error, `ID text`, with its status; the service's text
    /// kept as the message's.
    fn service(&self, e: &str) -> Refusal {
        let (id, text) = e.split_once(' ').unwrap_or((e, ""));
        let status = match id {
            "CPF3520" | "CPF9801" | "CPF2110" | "CPF2105" | "TCP1A73" => Status::NotFound,
            "CPF3502" | "CPF9802" | "TCP1A74" => Status::NotAuthorized,
            "CPF3503" => Status::InUse,
            _ => Status::Error,
        };
        // The service's own messages come as their data alone.
        if id.starts_with("TCP1A7") {
            return Refusal {
                status,
                message: self.message(id, &[text]),
            };
        }
        let mut message = self.message(id, &[]);
        if !text.is_empty() {
            message.text = text.to_string();
        }
        Refusal { status, message }
    }

    /// Run `f` on the session, `Error` with `TCP1A71` without one.
    fn session<T>(&self, f: impl FnOnce(&mut Session) -> Result<T, Fail>) -> Result<T, Refusal> {
        let r = {
            let mut g = lock(&SESSION);
            match g.as_mut() {
                Some(s) => f(s),
                None => Err((Status::Error, NO_SESSION.into(), Vec::new())),
            }
        };
        WAKE.notify_all();
        r.map_err(|e| self.refuse(e))
    }

    fn order(&self, o: DbgOrder) -> Result<(), Refusal> {
        self.link
            .borrow_mut()
            .with(|l| l.srvjob_order(o))
            .map_err(|e| self.service(&e))
    }

    /// End the session: a stopped job goes on, the other job is let go,
    /// the entry points removed.
    fn close(&self) {
        let Some(s) = lock(&SESSION).take() else {
            return;
        };
        WAKE.notify_all();
        if s.target.is_some() || s.attached.is_some() {
            let _ = self.link.borrow_mut().with(Client::srvjob_end);
        }
        if !s.entries.is_empty() {
            let _ = self
                .link
                .borrow_mut()
                .with(|l| l.srvent_remove(String::new(), "*ALL".into()));
        }
        lock(&REMOTE).clear();
    }

    /// The serviced job's news into [`REMOTE`].
    fn pull(&self) {
        let new = self.link.borrow_mut().with(Client::srvjob_events);
        lock(&REMOTE).extend(new);
    }

    /// The first piece of news `pick` takes, waited for.
    fn answer<T>(&self, pick: impl Fn(&DbgEvent) -> Option<T>) -> Result<T, Refusal> {
        let until = Instant::now() + ANSWER;
        loop {
            {
                let mut q = lock(&REMOTE);
                if let Some(i) = q.iter().position(|e| pick(e).is_some()) {
                    let e = q.remove(i).expect("found");
                    return Ok(pick(&e).expect("picked"));
                }
            }
            if Instant::now() > until {
                return Err(self.refuse((Status::Error, "CPF3504".into(), Vec::new())));
            }
            self.pull();
            std::thread::sleep(POLL);
        }
    }

    /// Ask the stopped connection job; `None` when it is not stopped.
    fn ask_own<T>(
        &self,
        make: impl FnOnce(mpsc::Sender<T>) -> Order,
    ) -> Option<Result<T, Refusal>> {
        let (tx, rx) = mpsc::channel();
        {
            let mut g = lock(&SESSION);
            let s = g.as_mut()?;
            if !s.own_stopped {
                return None;
            }
            s.orders.push_back(make(tx));
        }
        WAKE.notify_all();
        Some(
            rx.recv_timeout(ANSWER)
                .map_err(|_| self.refuse((Status::Error, "CPD9CCC".into(), vec!["Debug".into()]))),
        )
    }

    /// Which job is stopped: the connection's own, the other, or none.
    fn stopped() -> (bool, bool) {
        lock(&SESSION)
            .as_ref()
            .map_or((false, false), |s| (s.own_stopped, s.remote_stopped))
    }

    /// The stopped job's call stack.
    fn frames(&self, what: &str) -> Result<Vec<DbgFrame>, Refusal> {
        match Self::stopped() {
            (true, _) => self
                .ask_own(Order::Frames)
                .unwrap_or_else(|| Err(self.not_stopped(what))),
            (false, true) => {
                self.order(DbgOrder::Frames)?;
                self.answer(|e| match e {
                    DbgEvent::Frames { list } => Some(
                        list.iter()
                            .map(|f| DbgFrame {
                                program: f.program.clone(),
                                module: f.module.clone(),
                                procedure: f.procedure.clone(),
                                stmt: f.stmt,
                                debug: f.debug,
                            })
                            .collect(),
                    ),
                    _ => None,
                })
            }
            _ => Err(self.not_stopped(what)),
        }
    }

    /// `CPD9CCC` naming `what`; `TCP1A71` without a session.
    fn not_stopped(&self, what: &str) -> Refusal {
        if lock(&SESSION).is_none() {
            return self.refuse((Status::Error, NO_SESSION.into(), Vec::new()));
        }
        self.refuse((Status::Error, "CPD9CCC".into(), vec![what.into()]))
    }

    /// `EVAL` in a frame of the stopped job.
    fn eval_at(&self, frame: u32, expr: &str) -> Result<Result<String, String>, Refusal> {
        match Self::stopped() {
            (true, _) => self
                .ask_own(|tx| Order::Eval(frame, expr.to_string(), tx))
                .unwrap_or_else(|| Err(self.not_stopped("Eval"))),
            (false, true) => {
                self.order(DbgOrder::EvalAt {
                    frame,
                    expr: expr.into(),
                })?;
                self.answer(|e| match e {
                    DbgEvent::Value { text, error } if error.is_empty() => Some(Ok(text.clone())),
                    DbgEvent::Value { error, .. } => Some(Err(error.clone())),
                    _ => None,
                })
            }
            _ => Err(self.not_stopped("Eval")),
        }
    }

    /// The other job's breakpoints from the session's.
    fn sync_remote(&self) {
        let list = lock(&SESSION)
            .as_ref()
            .filter(|s| s.target.is_some() || s.attached.is_some())
            .map(Session::core_breakpoints);
        if let Some(list) = list {
            let _ = self.order(DbgOrder::Breakpoints { list });
        }
    }

    /// A program described for the session.
    fn describe(&self, lib: &str, name: &str) -> Result<Program, Refusal> {
        let p = self
            .link
            .borrow_mut()
            .with(|l| l.dbg_program(lib.into(), name.into()))
            .map_err(|e| self.service(&e))?;
        Ok(Program {
            key: format!("{}/{}", p.lib, p.name),
            modules: p.modules,
        })
    }

    /// A stop of the other job, as the client sees it; `None` when a
    /// breakpoint's condition does not hold (the job goes on).
    fn remote_stop(&self, program: &str, stmt: u32) -> Option<Stopped> {
        let (bkpt, entry, stepping, job) = {
            let g = lock(&SESSION);
            let s = g.as_ref()?;
            let job = s.target.clone().or_else(|| s.attached.clone())?;
            (
                s.bkpt_at(program, stmt).cloned(),
                s.entry_pending,
                s.stepping,
                job,
            )
        };
        if let Some(c) = bkpt
            .as_ref()
            .and_then(|b| b.cond.as_deref())
            .and_then(dbg::cond::parse)
        {
            let shown = self
                .eval_at(0, &c.var)
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            if !c.holds(&shown) && !entry && !stepping {
                let _ = self.order(DbgOrder::Resume {
                    step: false,
                    into: false,
                });
                return None;
            }
        }
        let reason = if entry {
            StopReason::ServiceEntry
        } else if bkpt.is_some() {
            StopReason::Breakpoint
        } else {
            StopReason::Step
        };
        let top = self
            .frames("Stack")
            .ok()
            .and_then(|f| f.into_iter().next())
            .unwrap_or_else(|| DbgFrame {
                program: program.into(),
                stmt,
                ..DbgFrame::default()
            });
        let mut g = lock(&SESSION);
        let s = g.as_mut()?;
        s.entry_pending = false;
        Some(Stopped {
            job: job_of(&job),
            reason,
            frame: s.frame(&top),
        })
    }

    /// What the storage service says of the other job: a job a service
    /// entry point attached, or the end of the job debugged.
    fn watch_remote(&self, out: &mut Vec<News>) {
        let (target, attached, entries) = {
            let g = lock(&SESSION);
            let Some(s) = g.as_ref() else {
                return;
            };
            (s.target.clone(), s.attached.clone(), s.entries.clone())
        };
        if target.is_none() && attached.is_none() && entries.is_empty() {
            return;
        }
        let serving = self.link.borrow_mut().with(Client::srvjob_serving);
        match (target, attached) {
            (Some(t), _) if serving.is_empty() => {
                // The job ended.
                let m = self.message(JOB_ENDED, &[&t]);
                self.close();
                out.push(News::Ended(Some(m)));
            }
            (None, None) if !serving.is_empty() => {
                // An entry point attached a job: its programs go on the
                // session, with the breakpoints.
                let mut programs = Vec::new();
                for key in &entries {
                    let (lib, name) = key.split_once('/').unwrap_or(("*LIBL", key));
                    if let Ok(p) = self.describe(lib, name) {
                        programs.push(p);
                    }
                }
                if let Some(s) = lock(&SESSION).as_mut() {
                    s.attached = Some(serving);
                    s.entry_pending = true;
                    for p in programs {
                        if s.program(&p.key).is_none() {
                            s.programs.push(p);
                        }
                    }
                }
                self.sync_remote();
                let keys: Vec<String> = lock(&SESSION)
                    .as_ref()
                    .map(|s| s.programs.iter().map(|p| p.key.clone()).collect())
                    .unwrap_or_default();
                // The entries' programs are under debug there already.
                for program in keys.into_iter().filter(|k| !entries.contains(k)) {
                    let _ = self.order(DbgOrder::Start {
                        program,
                        updprod: false,
                    });
                }
            }
            (None, Some(_)) if serving.is_empty() => {
                // The attached job ended; the session goes on.
                if let Some(s) = lock(&SESSION).as_mut() {
                    s.attached = None;
                    s.remote_stopped = false;
                }
            }
            _ => {}
        }
    }
}

impl DebugHost for Thread {
    fn start(&mut self, req: &Start) -> Result<Started, Refusal> {
        if lock(&SESSION).is_some() {
            return Err(self.refuse((Status::InUse, SESSION_ACTIVE.into(), Vec::new())));
        }
        let target = match &req.target {
            DebugTarget::Own => None,
            DebugTarget::Job { job } => {
                let name: Vec<&str> = [job.number.trim(), job.user.trim(), job.name.trim()]
                    .into_iter()
                    .filter(|p| !p.is_empty())
                    .collect();
                let q = self
                    .link
                    .borrow_mut()
                    .with(|l| l.srvjob_start(name.join("/")))
                    .map_err(|e| self.service(&e))?;
                Some(q)
            }
        };
        let number = NUMBERS.fetch_add(1, Ordering::SeqCst) + 1;
        lock(&REMOTE).clear();
        *lock(&SESSION) = Some(Session {
            target,
            ..Session::default()
        });
        Ok(Started { session: number })
    }

    fn end(&mut self) {
        self.close();
    }

    fn active(&mut self) -> bool {
        lock(&SESSION).is_some()
    }

    fn add_program(&mut self, req: &AddProgram) -> Result<ProgramAdded, Refusal> {
        self.session(|_| Ok(()))?;
        let ty = req.r#type.trim().to_ascii_uppercase();
        if !(ty.is_empty() || ty == "*PGM" || ty == "*SRVPGM") {
            return Err(self.refuse((Status::InvalidRequest, "CPD0084".into(), vec![ty])));
        }
        let p = self.describe(&req.program.library, &req.program.name)?;
        if p.modules.iter().all(|m| m.view == "*NONE") {
            let name = p.key.rsplit('/').next().unwrap_or_default().to_string();
            return Err(self.refuse((Status::Error, NO_DEBUG_DATA.into(), vec![name])));
        }
        let modules = p
            .modules
            .iter()
            .map(|m| DebugModule {
                name: m.name.clone(),
                views: views(&m.view),
            })
            .collect();
        let key = p.key.clone();
        let remote = self.session(|s| {
            if let Some(i) = s.programs.iter().position(|o| o.key == p.key) {
                s.programs[i] = p;
            } else {
                s.programs.push(p);
            }
            Ok(s.target.is_some() || s.attached.is_some())
        })?;
        if remote {
            self.order(DbgOrder::Start {
                program: key,
                updprod: false,
            })?;
        }
        Ok(ProgramAdded { modules })
    }

    fn set_breakpoints(&mut self, req: &SetBreakpoints) -> Result<BreakpointsSet, Refusal> {
        let want = req.program.name.trim().to_ascii_uppercase();
        let lib = req.program.library.trim().to_ascii_uppercase();
        let set = self.session(|s| {
            let p = s
                .programs
                .iter()
                .find(|p| {
                    let (l, n) = p.key.split_once('/').unwrap_or(("", &p.key));
                    n == want && (lib.is_empty() || lib.starts_with('*') || lib == l)
                })
                .cloned()
                .ok_or_else(|| {
                    (
                        Status::NotFound,
                        "CPF9801".into(),
                        vec![want.clone(), lib.clone()],
                    )
                })?;
            let module = req.module.trim().to_ascii_uppercase();
            let m = p
                .modules
                .iter()
                .find(|m| module.is_empty() || m.name == module)
                .ok_or_else(|| {
                    (
                        Status::NotFound,
                        "CPF9801".into(),
                        vec![module.clone(), lib.clone()],
                    )
                })?;
            let mut list = Vec::new();
            let mut set = Vec::new();
            for b in &req.lines {
                let (line, stmt) = landing(m, b.line)
                    .ok_or_else(|| (Status::Error, "CPD9CCB".into(), vec![b.line.to_string()]))?;
                if let Some(c) = &b.condition
                    && dbg::cond::parse(c).is_none()
                {
                    return Err((Status::Error, "CPF9C59".into(), vec![c.clone()]));
                }
                list.push(Bkpt {
                    program: p.key.clone(),
                    module: m.name.clone(),
                    stmt,
                    cond: b.condition.clone(),
                });
                set.push(BreakpointSet { line, stmt });
            }
            s.bkpts
                .retain(|b| !(b.program == p.key && b.module == m.name));
            s.bkpts.extend(list);
            Ok(set)
        })?;
        self.sync_remote();
        Ok(BreakpointsSet { set })
    }

    fn resume(&mut self, req: &Resume) -> Result<(), Refusal> {
        let (own, remote) = Self::stopped();
        if own {
            self.session(|s| {
                s.orders.push_back(Order::Resume(req.how));
                Ok(())
            })?;
            return Ok(());
        }
        if !remote {
            return Err(self.not_stopped("Resume"));
        }
        self.session(|s| {
            s.remote_stopped = false;
            s.stepping = req.how != ResumeHow::Go;
            Ok(())
        })?;
        self.order(match req.how {
            ResumeHow::Go => DbgOrder::Resume {
                step: false,
                into: false,
            },
            ResumeHow::StepOver => DbgOrder::Resume {
                step: true,
                into: false,
            },
            ResumeHow::StepInto => DbgOrder::Resume {
                step: true,
                into: true,
            },
            ResumeHow::StepOut => DbgOrder::StepOut,
        })
    }

    fn stack(&mut self) -> Result<Frames, Refusal> {
        self.session(|_| Ok(()))?;
        let frames = self.frames("Stack")?;
        let g = lock(&SESSION);
        let Some(s) = g.as_ref() else {
            return Ok(Frames::default());
        };
        Ok(Frames {
            frames: frames
                .iter()
                .filter(|f| !f.program.is_empty())
                .map(|f| s.frame(f))
                .collect(),
        })
    }

    fn variables(&mut self, req: &Variables) -> Result<VariableList, Refusal> {
        self.session(|_| Ok(()))?;
        let parent = req.parent.clone();
        let vals: Result<Vec<DbgValue>, String> = match Self::stopped() {
            (true, _) => self
                .ask_own(|tx| Order::Vars(req.frame, parent, tx))
                .unwrap_or_else(|| Err(self.not_stopped("Variables")))?,
            (false, true) => {
                self.order(DbgOrder::Vars {
                    frame: req.frame,
                    parent: parent.unwrap_or_default(),
                })?;
                self.answer(|e| match e {
                    DbgEvent::Vars { list, error } if error.is_empty() => Some(Ok(list
                        .iter()
                        .map(|v| DbgValue {
                            name: v.name.clone(),
                            kind: v.kind.clone(),
                            value: v.value.clone(),
                            expandable: v.expandable,
                        })
                        .collect())),
                    DbgEvent::Vars { error, .. } => Some(Err(error.clone())),
                    _ => None,
                })?
            }
            _ => return Err(self.not_stopped("Variables")),
        };
        match vals {
            Ok(v) => Ok(VariableList {
                variables: v
                    .into_iter()
                    .map(|v| Variable {
                        name: v.name,
                        r#type: v.kind,
                        value: v.value,
                        expandable: v.expandable,
                    })
                    .collect(),
            }),
            Err(_) => Err(self.refuse((
                Status::InvalidRequest,
                "TCP1A77".into(),
                vec![req.frame.to_string()],
            ))),
        }
    }

    fn eval(&mut self, req: &Eval) -> Result<Evaluated, Refusal> {
        self.session(|_| Ok(()))?;
        match self.eval_at(req.frame, &req.expression)? {
            Ok(value) => return Ok(Evaluated { value }),
            Err(why) if why.starts_with(mi::debug::NO_FRAME) => {
                return Err(self.refuse((
                    Status::InvalidRequest,
                    "TCP1A77".into(),
                    vec![req.frame.to_string()],
                )));
            }
            Err(_) => {}
        }
        let name = req
            .expression
            .split(':')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        Err(self.refuse((Status::Error, "CPF7E12".into(), vec![name])))
    }

    fn set_service_entry(&mut self, req: &SetServiceEntry) -> Result<(), Refusal> {
        self.session(|_| Ok(()))?;
        let key = self
            .link
            .borrow_mut()
            .with(|l| {
                l.srvent_set(
                    req.program.library.clone(),
                    req.program.name.clone(),
                    req.user.clone(),
                )
            })
            .map_err(|e| self.service(&e))?;
        self.session(|s| {
            if !s.entries.contains(&key) {
                s.entries.push(key);
            }
            Ok(())
        })
    }

    fn remove_service_entry(&mut self, req: &RemoveServiceEntry) -> Result<(), Refusal> {
        self.session(|_| Ok(()))?;
        self.link
            .borrow_mut()
            .with(|l| l.srvent_remove(req.program.library.clone(), req.program.name.clone()))
            .map_err(|e| self.service(&e))?;
        let name = req.program.name.trim().to_ascii_uppercase();
        self.session(|s| {
            s.entries
                .retain(|k| k.rsplit('/').next() != Some(name.as_str()));
            Ok(())
        })
    }

    fn news(&mut self) -> Vec<News> {
        let mut out: Vec<News> = match lock(&SESSION).as_mut() {
            Some(s) => s.news.drain(..).collect(),
            None => return Vec::new(),
        };
        self.watch_remote(&mut out);
        let remote = lock(&SESSION)
            .as_ref()
            .is_some_and(|s| s.target.is_some() || s.attached.is_some());
        if !remote {
            return out;
        }
        self.pull();
        let stops: Vec<(String, u32)> = {
            let mut q = lock(&REMOTE);
            let mut stops = Vec::new();
            q.retain(|e| match e {
                DbgEvent::Stopped { program, stmt, .. } => {
                    stops.push((program.clone(), *stmt));
                    false
                }
                // `Start`'s answers and the job going on: nobody waits.
                DbgEvent::Running | DbgEvent::Answer { .. } => false,
                _ => true,
            });
            stops
        };
        for (program, stmt) in stops {
            if let Some(s) = lock(&SESSION).as_mut() {
                s.remote_stopped = true;
            }
            match self.remote_stop(&program, stmt) {
                Some(stopped) => out.push(News::Stopped(stopped)),
                None => {
                    if let Some(s) = lock(&SESSION).as_mut() {
                        s.remote_stopped = false;
                    }
                }
            }
        }
        out
    }
}
