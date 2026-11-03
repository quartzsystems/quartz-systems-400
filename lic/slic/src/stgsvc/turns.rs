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
use crate::ipl::storage;
use crate::ipl::system::System;

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
    /// Statements in progress, each with its connection.
    fibers: Vec<(u64, Fiber)>,
    /// A close waits: statements in progress end at their next yield.
    closing: bool,
}

thread_local! {
    /// The service while a fiber runs.
    static PARKED: RefCell<Option<Box<Svc>>> = const { RefCell::new(None) };
}

fn park(svc: Box<Svc>) {
    PARKED.with(|p| *p.borrow_mut() = Some(svc));
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
    let wait = if top { wait } else { Duration::ZERO };
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
    while svc.fibers.len() < MAX_FIBERS
        && let Some((id, req)) = svc.queued.pop_front()
    {
        svc = start(svc, id, req);
    }
    turns(svc)
}

/// Answer one request in the ordinary way.
fn answer(svc: &mut Svc, i: usize, req: Request) {
    let close = matches!(req, Request::Close {});
    let reply = served::handle(&mut svc.system, &mut svc.conns, i, req);
    let _ = svc.conns[i].chan.send_raw(&reply);
    if close {
        std::process::exit(0);
    }
}

/// Whether a request is a statement that may take turns: the engine
/// decides at its safe points whether it does.
fn long(svc: &Svc, i: usize, req: &Request) -> bool {
    let Request::SqlRun { desc, .. } = req else {
        return false;
    };
    !svc.closing
        && svc.conns[i].ctx.is_some()
        && Desc::decode(desc).is_some_and(|d| {
            matches!(d.op, Op::Query | Op::Open | Op::Exec | Op::ExecImm | Op::Execute)
        })
}

/// Give each statement in progress one turn: until its next yield or
/// its end.
fn turns(mut svc: Box<Svc>) -> Box<Svc> {
    for _ in 0..svc.fibers.len() {
        let (id, mut f) = svc.fibers.remove(0);
        park(svc);
        let ended = f.resume();
        svc = unpark();
        if ended {
            f.recycle();
        } else {
            svc.fibers.push((id, f));
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
}

/// Start `sql_run` as `served::handle` would, on a fiber with the
/// yielder letting the others in, and give it its first turn.
fn start(mut svc: Box<Svc>, id: u64, req: Request) -> Box<Svc> {
    let Some(i) = svc.index(id) else {
        svc.busy.retain(|b| *b != id);
        return svc;
    };
    let stack = match fiber::stack() {
        Ok(s) if !svc.closing => s,
        Ok(_) => {
            // Too late to take turns: answered as any request.
            svc.busy.retain(|b| *b != id);
            answer(&mut svc, i, req);
            return svc;
        }
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
    let Request::SqlRun { scope, desc, inputs } = req else {
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
    let turn = Rc::new(Turn {
        id,
        entered: Cell::new(entered),
    });
    engine.ctx.yielder = Some(yielder(Rc::clone(&turn)));
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
        park(finish(unpark(), &turn, engine, &reply));
    };
    let mut f = Fiber::on(stack, body);
    park(svc);
    let ended = f.resume();
    svc = unpark();
    if ended {
        f.recycle();
    } else {
        svc.fibers.push((id, f));
    }
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
