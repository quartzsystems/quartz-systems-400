// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Long statements take turns with every other request. The serve loop
//! is one thread over one store; a query that reads a large table would
//! hold every job's requests until it ended. So an embedded statement
//! that only reads runs with a yielder: between batches of its outermost
//! scan the engine gives the store back and calls it, and the yielder
//! puts the service as it is between requests, answers what is ready on
//! the other connections, and sets the statement's job up again. While a
//! statement runs the service is parked here, owned by nobody's frame,
//! so the yield can reach it. `docs/STORAGE_SERVICE.md` "Long
//! statements" has the rules.

use std::cell::{Cell, RefCell};
use std::os::unix::net::UnixListener;
use std::rc::Rc;
use std::time::Duration;

use ipc::chan::Chan;
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use sql::embed::{Desc, Embedded};
use stg::Request;

use super::served::{self, Conn};
use super::{cmn, ifsvol};
use crate::ipl::storage;
use crate::ipl::system::System;

/// Long statements in progress at once, each inside the one before's
/// turn; past it a statement runs to its end without yielding.
const MAX_DEPTH: u32 = 4;

/// What the serve loop owns.
pub struct Svc {
    /// The system.
    pub system: System,
    /// `slic`'s connection and the jobs'.
    pub conns: Vec<Conn>,
    listener: UnixListener,
    /// Connections whose request is in progress or deferred: not read.
    busy: Vec<u64>,
    /// Requests left for the top of the loop (`close`).
    deferred: Vec<(u64, Request)>,
    /// Long statements in progress.
    depth: u32,
    /// A close waits: statements in progress end at their next yield.
    closing: bool,
}

thread_local! {
    /// The service while a long statement runs.
    static PARKED: RefCell<Option<Box<Svc>>> = const { RefCell::new(None) };
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
            depth: 0,
            closing: false,
        })
    }

    fn index(&self, id: u64) -> Option<usize> {
        self.conns.iter().position(|c| c.id == id)
    }
}

/// One pass of the loop: wait up to `wait` for a connection, accept,
/// answer every connection that is ready. At the top (no statement in
/// progress) deferred requests go first and background disk work runs.
pub fn round(mut svc: Box<Svc>, wait: Duration) -> Box<Svc> {
    let top = svc.depth == 0;
    if top {
        for (id, req) in std::mem::take(&mut svc.deferred) {
            svc.busy.retain(|b| *b != id);
            if let Some(i) = svc.index(id) {
                answer(&mut svc, i, req);
            }
        }
    }
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
        if !top && matches!(req, Request::Close {}) {
            // The close ends every job's database use: not under a
            // statement still running.
            svc.closing = true;
            svc.busy.push(id);
            svc.deferred.push((id, req));
            continue;
        }
        if long(&svc, i, &req) {
            svc = run_long(svc, id, req);
        } else {
            answer(&mut svc, i, req);
        }
    }
    if grouped {
        served::group(&mut svc.system, false);
    }
    svc
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

/// Whether a request is a statement that may take turns.
fn long(svc: &Svc, i: usize, req: &Request) -> bool {
    let Request::SqlRun { desc, .. } = req else {
        return false;
    };
    svc.depth < MAX_DEPTH
        && !svc.closing
        && svc.conns[i].ctx.is_some()
        && Desc::decode(desc).is_some_and(|d| sql::embed::may_yield(&d))
}

/// What a statement taking turns keeps across its yields.
struct Turn {
    /// The connection.
    id: u64,
    /// What setting its job up changed.
    entered: Cell<served::Entered>,
}

/// Run `sql_run` as `served::handle` would, with the service parked
/// while the engine runs and the yielder letting the others in.
fn run_long(mut svc: Box<Svc>, id: u64, req: Request) -> Box<Svc> {
    let user = super::security::as_user(&req);
    let i = svc.index(id).expect("ready");
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
            let _ = conns[i].chan.send_raw(&ipc::encode(&*w));
            return svc;
        }
    };
    let turn = Rc::new(Turn {
        id,
        entered: Cell::new(entered),
    });
    engine.ctx.yielder = Some(yielder(Rc::clone(&turn)));
    svc.busy.push(id);
    svc.depth += 1;
    PARKED.with(|p| *p.borrow_mut() = Some(svc));
    let req = sql::embed::Request {
        scope,
        desc,
        inputs: inputs.iter().map(sql::embed::from_wire).collect(),
    };
    let reply = if user {
        sec::guard::as_job(&session, || engine.run(&req))
    } else {
        engine.run(&req)
    };
    let mut svc = PARKED
        .with(|p| p.borrow_mut().take())
        .expect("the service is parked while a statement runs");
    svc.depth -= 1;
    svc.busy.retain(|b| *b != id);
    engine.ctx.yielder = None;
    let i = svc.index(id).expect("a busy connection stays");
    let Svc { system, conns, .. } = &mut *svc;
    let ctx = conns[i].ctx.as_mut().expect("a job");
    ctx.sql = Some(engine);
    served::leave(system, turn.entered.get());
    let who = served::charged(ctx);
    super::security::flush(system, Some(who));
    let _ = conns[i].chan.send_raw(&ipc::encode(&reply.to_wire()));
    svc
}

/// The statement's yield: the service as between requests, one round
/// without waiting, then the statement's job set up again. True ends
/// the statement (a close waits).
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
        let mut svc = sec::guard::as_system(|| round(svc, Duration::ZERO));
        let i = svc.index(turn.id).expect("a busy connection stays");
        let Svc { system, conns, .. } = &mut *svc;
        let ctx = conns[i].ctx.as_ref().expect("a job");
        turn.entered.set(served::enter(system, ctx));
        let stop = svc.closing;
        PARKED.with(|p| *p.borrow_mut() = Some(svc));
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
