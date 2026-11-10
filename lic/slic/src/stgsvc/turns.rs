// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Long statements take turns with every other request. The serve loop
//! is one thread over one store; a statement over a large table would
//! hold every job's requests until it ended. So an embedded query or
//! change runs on a fiber of its own with a yielder: at its safe points
//! the engine gives the store back and calls it, and the yielder puts
//! the service as it is between requests and suspends the fiber. The
//! loop then answers what is ready on the other connections and resumes
//! each statement in progress once, in turn, whichever started first.
//! While a fiber runs the service is parked here, so its yielder can
//! reach it. `docs/STORAGE_SERVICE.md` "Long statements" has the rules.
//!
//! A statement that needs a program called in its middle (a trigger
//! program) answers its job with the call plan and waits, not taking
//! turns, while the job's connection is read again: the program's own
//! requests are served, its own statements on fibers of their own. The
//! job's `Op::Called` named `midcall::ANSWER` resumes the latest
//! statement of that job waiting for one ("Trigger programs").

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::os::unix::net::UnixListener;
use std::rc::Rc;
use std::time::Duration;

use ipc::chan::Chan;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use sql::embed::{Desc, Embedded, Op};
use stg::Request;

use super::fiber::{self, Fiber};
use super::served::{self, Conn};
use super::{cmn, ifsvol};
use crate::cpp::longcmd;
use crate::ipl::storage;
use crate::ipl::system::System;
use crate::svc::ile::MainStore;

/// Statements in progress at once; past it a statement waits for one
/// to end before it starts.
const MAX_FIBERS: usize = 64;

/// What the serve loop owns.
pub struct Svc {
    /// The system.
    pub system: System,
    /// `slic`'s connection and the jobs'.
    pub conns: Vec<Conn>,
    listener: UnixListener,
    /// Connections whose request is in progress or deferred: not read.
    busy: Vec<u64>,
    /// Requests left for when no statement is in progress (`close`).
    deferred: Vec<(u64, Request)>,
    /// Long statements waiting for a fiber.
    queued: VecDeque<(u64, Request)>,
    /// Statements in progress, each with its turn.
    fibers: Vec<(Rc<Turn>, Fiber)>,
    /// A close waits: statements in progress end at their next yield.
    closing: bool,
    /// Connections that went while statements of theirs were in
    /// progress: ended when the last of them has.
    doomed: Vec<u64>,
    /// The number of the next turn.
    next_turn: u64,
    /// IFS requests waiting for another job's lease break, or polls for
    /// break orders (`super::ifs::parking`): their connections are busy.
    parked: Vec<(u64, Request, ifs::lease::defer::Deferral)>,
}

thread_local! {
    /// The service while a fiber runs.
    static PARKED: RefCell<Option<Box<Svc>>> = const { RefCell::new(None) };
    /// What [`load`] reports, as of the last pass of the loop.
    static LOAD: RefCell<Load> = const {
        RefCell::new(Load {
            fibers: Vec::new(),
            queued: 0,
        })
    };
}

/// The statements in progress and queued, as of the last pass of the
/// loop (`txn stats`).
#[derive(Clone, Debug, Default)]
pub struct Load {
    /// Each statement in progress: its job's number, and whether it
    /// waits for its job.
    pub fibers: Vec<(u32, bool)>,
    /// Statements waiting for a fiber.
    pub queued: usize,
}

/// The most statements in progress at once.
pub const MOST: usize = MAX_FIBERS;

/// The statements as of the last pass.
pub fn load() -> Load {
    LOAD.with(|l| l.borrow().clone())
}

fn note_load(svc: &Svc) {
    let job = |id: u64| {
        svc.index(id)
            .and_then(|i| svc.conns[i].ctx.as_ref())
            .map_or(0, |c| c.job.number)
    };
    let l = Load {
        fibers: svc
            .fibers
            .iter()
            .map(|(t, _)| (job(t.id), t.waiting.get()))
            .collect(),
        queued: svc.queued.len(),
    };
    LOAD.with(|s| *s.borrow_mut() = l);
}

fn park(svc: Box<Svc>) {
    PARKED.with(|p| *p.borrow_mut() = Some(svc));
}

/// Run `f` over the parked service's system: while a statement takes
/// its turn on a fiber. `None` when nothing is parked.
pub fn with_parked<T>(f: impl FnOnce(&mut System) -> T) -> Option<T> {
    PARKED.with(|p| {
        p.try_borrow_mut()
            .ok()
            .and_then(|mut b| b.as_mut().map(|svc| f(&mut svc.system)))
    })
}

/// The log of active job `job` (`number/user/name`, `*` for `me`) and
/// its qualified name, read from the parked service's connections;
/// `None` when nothing is parked.
pub fn job_log_of(job: &str, me: u32) -> Option<sql::exec::catalog::services::host::JobLog> {
    PARKED.with(|p| {
        let b = p.try_borrow().ok()?;
        let svc = b.as_ref()?;
        let number = if job == "*" {
            Some(me)
        } else {
            wm::job::JobId::parse(job).map(|j| j.number)
        };
        let found = number.and_then(|n| {
            svc.conns
                .iter()
                .filter_map(|c| c.ctx.as_ref())
                .find(|c| c.job.number == n && !c.thread)
        });
        Some(match found {
            Some(c) => sql::exec::catalog::services::host::JobLog::Found(
                c.job.qualified(),
                super::joblog::entries(c),
            ),
            None => sql::exec::catalog::services::host::JobLog::NoSuchJob,
        })
    })
}

/// The security principal of job `number`'s connection, from the
/// parked service; `None` when nothing is parked or no such job.
pub fn principal_of(number: u32) -> Option<sec::auth::principal::Principal> {
    PARKED.with(|p| {
        let b = p.try_borrow().ok()?;
        let svc = b.as_ref()?;
        let c = svc
            .conns
            .iter()
            .filter_map(|c| c.ctx.as_ref())
            .find(|c| c.job.number == number && !c.thread)?;
        Some(c.session.borrow().principal.clone())
    })
}

fn unpark() -> Box<Svc> {
    PARKED
        .with(|p| p.borrow_mut().take())
        .expect("the service is parked while a fiber runs")
}

impl Svc {
    /// The loop's state.
    pub fn new(system: System, conns: Vec<Conn>, listener: UnixListener) -> Box<Self> {
        Box::new(Self {
            system,
            conns,
            listener,
            busy: Vec::new(),
            deferred: Vec::new(),
            queued: VecDeque::new(),
            fibers: Vec::new(),
            closing: false,
            doomed: Vec::new(),
            next_turn: 0,
            parked: Vec::new(),
        })
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.conns.iter().position(|c| c.id == id)
    }
}

/// One pass of the loop: wait up to `wait` for a connection (not at all
/// while statements are in progress), accept, answer every connection
/// that is ready, then give each statement in progress one turn. With
/// none in progress deferred requests go first and background disk work
/// runs.
pub fn round(mut svc: Box<Svc>, wait: Duration) -> Box<Svc> {
    let top = svc.fibers.is_empty();
    if top {
        for (id, req) in std::mem::take(&mut svc.deferred) {
            svc.busy.retain(|b| *b != id);
            if let Some(i) = svc.index(id) {
                answer(&mut svc, i, req);
            }
        }
    }
    // With statements in progress the loop does not wait, unless every
    // one waits for a record another job holds (then a little) or for
    // its job's answer (then as long as at the top).
    let runnable = svc.fibers.iter().filter(|(t, _)| !t.waiting.get()).count();
    let wait = if top || runnable == 0 {
        wait
    } else if sql::exec::record_waits() >= runnable {
        Duration::from_millis(10).min(wait)
    } else {
        Duration::ZERO
    };
    let wait = parked_wait(&svc, wait);
    let (accept, ready) = wait_ready(&svc, wait);
    if top {
        // Background disk work (device parity, a mirror resume) runs in
        // turns between requests, short ones while jobs wait.
        let slice = if accept || !ready.is_empty() {
            storage::PARITY_SLICE_BUSY
        } else {
            storage::PARITY_SLICE
        };
        storage::tick_with(&mut svc.system, slice);
        ifsvol::tick(&mut svc.system);
        cmn::tick(&mut svc.system);
        super::perfcol::tick(&mut svc.system);
    }
    if accept {
        match svc.listener.accept() {
            Ok((s, _)) => svc.conns.push(Conn::new(Chan::new(s), None)),
            Err(e) => eprintln!("qsstg: accept: {e}"),
        }
    }
    let mut grouped = false;
    for id in ready.into_iter().rev() {
        let Some(i) = svc.index(id) else {
            continue;
        };
        let frame = svc.conns[i].chan.recv_raw();
        let Ok(req) = frame.and_then(|b| ipc::decode::<Request>(&b)) else {
            if svc.fibers.iter().any(|(t, _)| t.id == id) {
                // Its statements waiting for it end first.
                for (t, _) in &svc.fibers {
                    if t.id == id {
                        t.waiting.set(false);
                    }
                }
                svc.busy.push(id);
                svc.doomed.push(id);
                continue;
            }
            let c = svc.conns.remove(i);
            if !grouped {
                served::group(&mut svc.system, true);
                grouped = true;
            }
            served::vanished(&mut svc.system, c);
            continue;
        };
        if grouped {
            served::group(&mut svc.system, false);
            grouped = false;
        }
        if !svc.fibers.is_empty() && matches!(req, Request::Close {}) {
            // The close ends every job's database use: not under a
            // statement still running.
            svc.closing = true;
            svc.busy.push(id);
            svc.deferred.push((id, req));
            continue;
        }
        if let Some(t) = waiting_for(&svc, id, &req) {
            // The answer to a call made in a statement's middle.
            if let Request::SqlRun { inputs, .. } = req {
                *t.answer.borrow_mut() = Some(inputs.iter().map(sql::embed::from_wire).collect());
            }
            t.waiting.set(false);
            svc.busy.push(id);
            continue;
        }
        if long(&svc, i, &req) {
            svc.busy.push(id);
            svc.queued.push_back((id, req));
        } else {
            answer(&mut svc, i, req);
        }
    }
    if grouped {
        served::group(&mut svc.system, false);
    }
    wake_parked(&mut svc);
    super::census::refresh(&svc.conns);
    end_doomed(&mut svc);
    while svc.fibers.len() < MAX_FIBERS
        && let Some((id, req)) = svc.queued.pop_front()
    {
        svc = start(svc, id, req);
    }
    note_load(&svc);
    turns(svc)
}

/// The statement of connection `id` that `req` answers: the latest of
/// those waiting for their job.
fn waiting_for(svc: &Svc, id: u64, req: &Request) -> Option<Rc<Turn>> {
    let Request::SqlRun { desc, .. } = req else {
        return None;
    };
    let d = Desc::decode(desc)?;
    if d.op != Op::Called || d.name != sql::exec::routine::midcall::ANSWER {
        return None;
    }
    svc.fibers
        .iter()
        .filter(|(t, _)| t.id == id && t.waiting.get())
        .max_by_key(|(t, _)| t.seq)
        .map(|(t, _)| Rc::clone(t))
}

/// End the connections that went once none of their statements is in
/// progress.
fn end_doomed(svc: &mut Svc) {
    for id in std::mem::take(&mut svc.doomed) {
        if svc.fibers.iter().any(|(t, _)| t.id == id) {
            svc.doomed.push(id);
            continue;
        }
        svc.busy.retain(|b| *b != id);
        if let Some(i) = svc.index(id) {
            let c = svc.conns.remove(i);
            served::vanished(&mut svc.system, c);
        }
    }
}

/// Answer one request in the ordinary way: an IFS operation that has to
/// wait for another job's lease break is parked instead.
fn answer(svc: &mut Svc, i: usize, req: Request) {
    answer_as(svc, i, req, true);
}

/// Answer a request, parking it when `park` and it defers.
fn answer_as(svc: &mut Svc, i: usize, req: Request, park: bool) {
    let close = matches!(req, Request::Close {});
    let keep = match &req {
        Request::IfsReq { op } if park && super::ifs::may_park(op) => Some(req.clone()),
        _ => None,
    };
    let (reply, deferral) = super::ifs::parking(keep.is_some(), || {
        served::handle(&mut svc.system, &mut svc.conns, i, req)
    });
    if let (Some(d), Some(req)) = (deferral, keep) {
        let id = svc.conns[i].id;
        svc.busy.push(id);
        svc.parked.push((id, req, d));
        return;
    }
    let _ = svc.conns[i].chan.send_raw(&reply);
    if close {
        std::process::exit(0);
    }
}

/// `wait`, or less when a parked request's time runs out sooner.
fn parked_wait(svc: &Svc, wait: Duration) -> Duration {
    let now = std::time::Instant::now();
    svc.parked
        .iter()
        .map(|(_, _, d)| d.until().saturating_duration_since(now))
        .fold(wait, Duration::min)
}

/// Answer the parked requests whose wait is over: the breaks they waited
/// for were acknowledged (or timed out and are forced), or the orders a
/// poll waited for came. One out of time runs without parking again.
fn wake_parked(svc: &mut Svc) {
    if svc.parked.is_empty() {
        return;
    }
    let now = std::time::Instant::now();
    let Some(leases) = super::ifs::leases() else {
        return;
    };
    leases.expire(now);
    for (id, req, d) in std::mem::take(&mut svc.parked) {
        let late = now >= d.until();
        if !late && !leases.ready(&d) {
            svc.parked.push((id, req, d));
            continue;
        }
        svc.busy.retain(|b| *b != id);
        if let Some(i) = svc.index(id) {
            answer_as(svc, i, req, !late);
        }
    }
}

/// Whether a request is a statement or a command that may take turns:
/// the engine (or the database) decides at its safe points whether it
/// does.
fn long(svc: &Svc, i: usize, req: &Request) -> bool {
    if svc.closing || svc.conns[i].ctx.is_none() {
        return false;
    }
    match req {
        Request::SqlRun { desc, .. } => Desc::decode(desc).is_some_and(|d| {
            matches!(
                d.op,
                Op::Query | Op::Open | Op::Exec | Op::ExecImm | Op::Execute
            )
        }),
        Request::RunCmd { name, .. } | Request::RunCmdCalling { name, .. } => {
            longcmd::NAMES.contains(&name.as_str())
        }
        _ => false,
    }
}

/// Give each statement in progress one turn: until its next yield or
/// its end.
fn turns(mut svc: Box<Svc>) -> Box<Svc> {
    for _ in 0..svc.fibers.len() {
        let (t, mut f) = svc.fibers.remove(0);
        if t.waiting.get() && !svc.closing {
            // Its job has not answered yet.
            svc.fibers.push((t, f));
            continue;
        }
        park(svc);
        let ended = f.resume();
        svc = unpark();
        if ended {
            f.recycle();
        } else {
            svc.fibers.push((t, f));
        }
    }
    svc
}

/// What a statement taking turns keeps across its yields.
struct Turn {
    /// The connection.
    id: u64,
    /// What setting its job up changed.
    entered: Cell<served::Entered>,
    /// The order turns began in.
    seq: u64,
    /// It waits for its job to answer a call made in its middle.
    waiting: Cell<bool>,
    /// The answer: the inputs of the job's `Op::Called`.
    answer: RefCell<Option<Vec<sql::Value>>>,
}

impl Turn {
    fn new(svc: &mut Svc, id: u64, entered: served::Entered) -> Rc<Self> {
        svc.next_turn += 1;
        Rc::new(Self {
            id,
            entered: Cell::new(entered),
            seq: svc.next_turn,
            waiting: Cell::new(false),
            answer: RefCell::new(None),
        })
    }
}

/// Start a long request on a fiber with the yielder letting the others
/// in, and give it its first turn.
fn start(mut svc: Box<Svc>, id: u64, req: Request) -> Box<Svc> {
    let Some(i) = svc.index(id) else {
        svc.busy.retain(|b| *b != id);
        return svc;
    };
    if svc.closing {
        // Too late to take turns: answered as any request.
        svc.busy.retain(|b| *b != id);
        answer(&mut svc, i, req);
        return svc;
    }
    if matches!(req, Request::RunCmd { .. } | Request::RunCmdCalling { .. }) {
        return start_cmd(svc, id, i, req);
    }
    start_sql(svc, id, i, req)
}

/// Start `sql_run` as `served::handle` would.
fn start_sql(mut svc: Box<Svc>, id: u64, i: usize, req: Request) -> Box<Svc> {
    let stack = match fiber::stack() {
        Ok(s) => s,
        Err(e) => {
            // No stack to be had: this one runs to its end.
            eprintln!("qsstg: no stack for a statement: {e}");
            svc.busy.retain(|b| *b != id);
            answer(&mut svc, i, req);
            return svc;
        }
    };
    let user = super::security::as_user(&req);
    let number = svc.conns[i].ctx.as_ref().map_or(0, |c| c.job.number);
    super::trace::request(number, &req);
    let Request::SqlRun {
        scope,
        desc,
        inputs,
    } = req
    else {
        unreachable!("long() said a statement");
    };
    let desc = Desc::decode(&desc).expect("long() decoded it");
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("long() said a job");
    *system.stats.aux_io.entry(number).or_default() += 1;
    let session = Rc::clone(&ctx.session);
    super::security::freshen(system, &session);
    let entered = served::enter(system, ctx);
    let shared = Rc::clone(&system.lent);
    let mut engine = match served::sql_session(&shared, system.locks.clone(), ctx) {
        Ok(_) => ctx.sql.take().expect("just made"),
        Err(w) => {
            served::leave(system, entered);
            svc.busy.retain(|b| *b != id);
            let _ = svc.conns[i].chan.send_raw(&ipc::encode(&*w));
            return svc;
        }
    };
    let turn = Turn::new(&mut svc, id, entered);
    let t2 = Rc::clone(&turn);
    engine.ctx.yielder = Some(yielder(Rc::clone(&turn)));
    // Trigger programs are called in the job, the statement waiting.
    engine.ctx.midcall = Some(mid_caller(Rc::clone(&turn), false));
    let req = sql::embed::Request {
        scope,
        desc,
        inputs: inputs.iter().map(sql::embed::from_wire).collect(),
    };
    let body = move || {
        let reply = if user {
            sec::guard::as_job(&session, || engine.run(&req))
        } else {
            engine.run(&req)
        };
        engine.ctx.yielder = None;
        engine.ctx.midcall = None;
        park(finish(unpark(), &turn, engine, &reply));
    };
    first_turn(svc, t2, Fiber::on(stack, body))
}

/// A fiber's first turn; one that has not ended goes with the others.
fn first_turn(mut svc: Box<Svc>, turn: Rc<Turn>, mut f: Fiber) -> Box<Svc> {
    park(svc);
    let ended = f.resume();
    svc = unpark();
    if ended {
        f.recycle();
    } else {
        svc.fibers.push((turn, f));
    }
    svc
}

/// Start `run_cmd` of a command that takes turns: what it reads from
/// the job prepared as `served::handle` would run the command, then its
/// work on a fiber.
#[allow(clippy::too_many_lines)]
fn start_cmd(mut svc: Box<Svc>, id: u64, i: usize, req: Request) -> Box<Svc> {
    let user = super::security::as_user(&req);
    let number = svc.conns[i].ctx.as_ref().map_or(0, |c| c.job.number);
    super::trace::request(number, &req);
    let calls = matches!(req, Request::RunCmdCalling { .. });
    let (Request::RunCmd {
        name,
        segments,
        specified,
        typed,
    }
    | Request::RunCmdCalling {
        name,
        segments,
        specified,
        typed,
    }) = req
    else {
        unreachable!("start() said a command");
    };
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("long() said a job");
    *system.stats.aux_io.entry(number).or_default() += 1;
    let session = Rc::clone(&ctx.session);
    super::security::freshen(system, &session);
    let as_user = |f: &mut dyn FnMut()| {
        if user {
            sec::guard::as_job(&session, f);
        } else {
            f();
        }
    };
    let mut prepared = None;
    as_user(&mut || {
        prepared = Some(served::long_command(
            system,
            ctx,
            (&name, segments.clone(), &specified, &typed),
        ));
    });
    let (work, segments) = match prepared.expect("ran") {
        Ok(v) => v,
        Err(reply) => {
            // Ended (or ran) at once.
            super::security::flush(system, Some(served::charged(ctx)));
            let _ = conns[i].chan.send_raw(&ipc::encode(&*reply));
            svc.busy.retain(|b| *b != id);
            return svc;
        }
    };
    let entered = served::enter(system, ctx);
    let stack = match fiber::stack() {
        Ok(s) => s,
        Err(e) => {
            // No stack to be had: this one runs to its end.
            eprintln!("qsstg: no stack for a command: {e}");
            let lender = MainStore::new(Rc::clone(&system.lent));
            let mut finish = None;
            as_user(&mut || {
                let w = longcmd::Work::new(&lender, Rc::new(|| false));
                finish = Some(work_once(&w, &mut None));
            });
            drop(lender);
            served::leave(system, entered);
            let mut reply = None;
            as_user(&mut || {
                reply = Some(served::long_finish(
                    system,
                    ctx,
                    finish.take().expect("ran"),
                    segments.clone(),
                ));
            });
            super::security::flush(system, Some(served::charged(ctx)));
            let _ = conns[i].chan.send_raw(&ipc::encode(&reply.expect("ran")));
            svc.busy.retain(|b| *b != id);
            return svc;
        }
    };
    let shared = Rc::clone(&system.lent);
    let turn = Turn::new(&mut svc, id, entered);
    let t2 = Rc::clone(&turn);
    let y = yielder(Rc::clone(&turn));
    // The job makes the calls the work needs in its middle.
    let mid = calls.then(|| mid_caller(Rc::clone(&turn), true));
    let mut work = Some(work);
    let body = move || {
        let lender = MainStore::new(shared);
        let mut w = longcmd::Work::new(&lender, y);
        if let Some(m) = mid {
            w = w.with_midcall(m);
        }
        let finish = if user {
            sec::guard::as_job(&session, || work_once(&w, &mut work))
        } else {
            work_once(&w, &mut work)
        };
        drop(w);
        drop(lender);
        park(finish_cmd(
            unpark(),
            &turn,
            (finish, segments),
            user.then_some(&session),
        ));
    };
    first_turn(svc, t2, Fiber::on(stack, body))
}

/// Run a command's work, once.
fn work_once(w: &longcmd::Work<'_>, work: &mut Option<longcmd::Long>) -> longcmd::Finish {
    match work.take() {
        Some(f) => f(w),
        None => Box::new(|_: &mut crate::ipl::system::System| stg::proto::CmdReply::default()),
    }
}

/// A command ended: the service as between requests, its end run as one
/// of the job's commands, the reply sent.
fn finish_cmd(
    mut svc: Box<Svc>,
    turn: &Turn,
    (finish, segments): (longcmd::Finish, Vec<Option<Vec<u8>>>),
    session: Option<&Rc<RefCell<sec::auth::cache::Session>>>,
) -> Box<Svc> {
    svc.busy.retain(|b| *b != turn.id);
    let i = svc.index(turn.id).expect("a busy connection stays");
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("a job");
    served::leave(system, turn.entered.get());
    let reply = match session {
        Some(s) => sec::guard::as_job(s, || served::long_finish(system, ctx, finish, segments)),
        None => served::long_finish(system, ctx, finish, segments),
    };
    let who = served::charged(ctx);
    super::security::flush(system, Some(who));
    let _ = conns[i].chan.send_raw(&ipc::encode(&reply));
    svc
}

/// A statement ended: its engine back with its job, the service as
/// between requests, the reply sent.
fn finish(
    mut svc: Box<Svc>,
    turn: &Turn,
    engine: sql::embed::Session,
    reply: &sql::embed::Reply,
) -> Box<Svc> {
    svc.busy.retain(|b| *b != turn.id);
    let i = svc.index(turn.id).expect("a busy connection stays");
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("a job");
    ctx.sql = Some(engine);
    served::leave(system, turn.entered.get());
    let who = served::charged(ctx);
    super::security::flush(system, Some(who));
    let _ = conns[i].chan.send_raw(&ipc::encode(&reply.to_wire()));
    svc
}

/// The statement's yield: the service as between requests, the fiber
/// suspended until the loop gives it its next turn, then the statement's
/// job set up again. True ends the statement (a close waits).
fn yielder(turn: Rc<Turn>) -> sql::exec::Yielder {
    Rc::new(move || {
        let Some(mut svc) = PARKED.with(|p| p.borrow_mut().take()) else {
            return false;
        };
        served::leave(&mut svc.system, turn.entered.get());
        // Its audit entries so far are its own.
        if let Some(i) = svc.index(turn.id)
            && let Some(ctx) = svc.conns[i].ctx.as_ref()
        {
            let who = served::charged(ctx);
            super::security::flush(&mut svc.system, Some(who));
        }
        park(svc);
        // The loop and the others run as the system; this statement's
        // session comes back with its turn.
        sec::guard::as_system(fiber::suspend);
        let mut svc = unpark();
        let i = svc.index(turn.id).expect("a busy connection stays");
        let Svc { system, conns, .. } = &mut *svc;
        let ctx = conns[i].ctx.as_ref().expect("a job");
        turn.entered.set(served::enter(system, ctx));
        let stop = svc.closing;
        park(svc);
        stop
    })
}

/// A statement's calls in its middle: the service as between requests,
/// the plan sent to its job as the reply to its request, and the fiber
/// suspended, not taking turns, until the job's answer (or a close)
/// lets it go on; then its job set up again. The answer, `None` when
/// there is none.
fn mid_caller(turn: Rc<Turn>, cmd: bool) -> sql::exec::routine::midcall::MidCaller {
    Rc::new(move |plan| {
        let mut svc = PARKED.with(|p| p.borrow_mut().take())?;
        served::leave(&mut svc.system, turn.entered.get());
        let sent = match svc.index(turn.id) {
            Some(i) if !svc.closing && !svc.doomed.contains(&turn.id) => {
                if let Some(ctx) = svc.conns[i].ctx.as_ref() {
                    let who = served::charged(ctx);
                    super::security::flush(&mut svc.system, Some(who));
                }
                // A command's job reads a command reply (`run_cmd_calling`).
                let bytes = if cmd {
                    ipc::encode(&stg::proto::CmdReply {
                        call: Some(plan.encode().into_bytes()),
                        ..stg::proto::CmdReply::default()
                    })
                } else {
                    ipc::encode(
                        &sql::embed::Reply {
                            call: Some(Box::new(plan)),
                            ..sql::embed::Reply::default()
                        }
                        .to_wire(),
                    )
                };
                svc.conns[i].chan.send_raw(&bytes).is_ok()
            }
            _ => false,
        };
        *turn.answer.borrow_mut() = None;
        if sent {
            // The job's connection is read again meanwhile.
            svc.busy.retain(|b| *b != turn.id);
            turn.waiting.set(true);
        }
        park(svc);
        if sent {
            sec::guard::as_system(fiber::suspend);
        }
        let mut svc = unpark();
        turn.waiting.set(false);
        if let Some(i) = svc.index(turn.id) {
            let Svc { system, conns, .. } = &mut *svc;
            if let Some(ctx) = conns[i].ctx.as_ref() {
                turn.entered.set(served::enter(system, ctx));
            }
        }
        park(svc);
        turn.answer.borrow_mut().take()
    })
}

/// Wait for the listener or a connection that is not busy to be
/// readable; whether to accept, and the connections to read.
fn wait_ready(svc: &Svc, wait: Duration) -> (bool, Vec<u64>) {
    let open: Vec<&Conn> = svc
        .conns
        .iter()
        .filter(|c| !svc.busy.contains(&c.id))
        .collect();
    let mut fds = Vec::with_capacity(open.len() + 1);
    fds.push(PollFd::new(&svc.listener, PollFlags::IN));
    for c in &open {
        fds.push(PollFd::new(c.chan.stream(), PollFlags::IN));
    }
    let t = Timespec {
        tv_sec: i64::try_from(wait.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(wait.subsec_nanos()),
    };
    if poll(&mut fds, Some(&t)).is_err() {
        return (false, Vec::new());
    }
    let hit = |f: &PollFd<'_>| {
        f.revents()
            .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
    };
    let accept = hit(&fds[0]);
    let ready = fds[1..]
        .iter()
        .zip(&open)
        .filter(|(f, _)| hit(f))
        .map(|(_, c)| c.id)
        .collect();
    (accept, ready)
}

/// Whether background disk work waits: the loop's wait at the top.
pub fn top_wait(system: &System) -> Duration {
    if storage::busy(system) {
        Duration::from_millis(2)
    } else {
        Duration::from_secs(5)
    }
}
