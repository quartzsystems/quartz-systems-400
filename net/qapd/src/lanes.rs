// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where requests go (docs/QAP.md, Multiplexing): `Hello` first and
//! once, then only the agreed version's operations; `Ping` answered at
//! once; `Command` requests to the job's own thread, one at a time;
//! every other operation to a secondary thread of the job, as many at
//! once as are outstanding, each thread made when none is free.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use ipc::Wire;
use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod, choose_version};
use qap_proto::ops::signon::{ChangePassword, Resume, SignOn, SignOnReply};
use qap_proto::ops::spool::{
    HoldSpooledFile, ListOutputQueues, ListSpooledFiles, MoveSpooledFile, ReadSpooledFile, SpooledFileRef,
};
use qap_proto::{Envelope, HELLO_ID, Op, Service, Status, VERSION};

use crate::Closed;
use crate::conn::{Answer, Shared, lock};
use crate::host::{Refusal, ThreadHost, Threads};
use crate::pump::In;
use crate::watch::{self, Watch};

/// From the dispatcher to the job's own thread.
pub(crate) enum ToMain {
    /// A `Command` request, in arrival order.
    Command(Op, Envelope),
    /// The connection is to close.
    Closed(Closed),
}

/// Route requests until the connection closes or `Hello` refuses it.
pub(crate) fn dispatch(shared: &Arc<Shared>, rx: &Receiver<In>, threads: &Threads, main: &Sender<ToMain>, watch: &Arc<Watch>) {
    let mut own = threads();
    let mut pool = Pool::new(Arc::clone(shared), Arc::clone(threads), main.clone(), Arc::clone(watch));
    while let Ok(i) = rx.recv() {
        match i {
            In::Request(req) => {
                if let Some(why) = route(shared, &mut *own, &mut pool, main, req) {
                    let _ = main.send(ToMain::Closed(why));
                    return;
                }
            }
            In::Closed(why) => {
                let _ = main.send(ToMain::Closed(why));
                return;
            }
        }
    }
    // The pump says why before it ends, or its panic is reported.
    let _ = main.send(ToMain::Closed(Closed::Broken("the connection's reader ended".into())));
}

/// Send one request on its way; `Some` when the connection is to close.
fn route(shared: &Shared, own: &mut dyn ThreadHost, pool: &mut Pool, main: &Sender<ToMain>, req: Envelope) -> Option<Closed> {
    if shared.cancelled_before(&req) {
        return None;
    }
    let op = req.operation();
    let Some(version) = shared.version() else {
        // The first request is Hello, ID 1; anything else closes.
        if op != Some(Op::Hello) || req.request_id != HELLO_ID {
            shared.reply(&req, Answer::status(Status::InvalidRequest));
            return Some(Closed::Refused);
        }
        return hello(shared, own, &req);
    };
    match op {
        None => shared.reply(&req, Answer::status(Status::InvalidRequest)),
        // An operation newer than the connection's version.
        Some(op) if op.since() > version => shared.reply(&req, Answer::status(Status::InvalidRequest)),
        Some(Op::Hello) => {
            shared.reply(&req, Answer::status(Status::InvalidRequest));
            return Some(Closed::Refused);
        }
        // Never behind anything.
        Some(Op::Ping) => shared.finish(
            &req,
            Answer::ok(&PingReply {
                server_time: own.now_ms(),
            }),
        ),
        Some(op) if !op.before_signon() && !shared.signed_on() => {
            shared.reply(&req, Answer::status(Status::NotSignedOn));
        }
        Some(op) if op.code().0 == Service::Command => {
            let _ = main.send(ToMain::Command(op, req));
        }
        Some(op) => pool.run(op, req),
    }
    None
}

fn hello(shared: &Shared, own: &mut dyn ThreadHost, req: &Envelope) -> Option<Closed> {
    let Ok(h) = req.body_as::<Hello>() else {
        shared.reply(req, Answer::status(Status::InvalidRequest));
        return Some(Closed::Refused);
    };
    let Some(version) = choose_version(h.min_version, h.max_version, VERSION) else {
        shared.reply(req, Answer::status(Status::VersionUnsupported));
        return Some(Closed::Refused);
    };
    shared.set_version(version);
    let id = own.system_id();
    let reply = HelloReply {
        version,
        system_name: id.system_name,
        release: id.release,
        services: Service::of_version(version),
        capabilities: Vec::new(),
        signon_methods: vec![SignOnMethod::Password],
        server_time: own.now_ms(),
        time_zone: id.time_zone,
        system_ccsid: id.ccsid,
    };
    shared.reply(req, Answer::ok(&reply));
    None
}

/// The secondary threads.
struct Pool {
    shared: Arc<Shared>,
    threads: Threads,
    work: Sender<(Op, Envelope)>,
    queue: Arc<Mutex<Receiver<(Op, Envelope)>>>,
    /// Threads made.
    made: usize,
    /// Requests handed over and not yet answered.
    busy: Arc<AtomicUsize>,
    /// Where a thread's panic goes: the connection closes.
    main: Sender<ToMain>,
    watch: Arc<Watch>,
}

impl Pool {
    fn new(shared: Arc<Shared>, threads: Threads, main: Sender<ToMain>, watch: Arc<Watch>) -> Self {
        let (work, queue) = mpsc::channel();
        Self {
            shared,
            threads,
            work,
            queue: Arc::new(Mutex::new(queue)),
            made: 0,
            busy: Arc::new(AtomicUsize::new(0)),
            main,
            watch,
        }
    }

    /// Run `req` on a free thread, made if none is.
    fn run(&mut self, op: Op, req: Envelope) {
        if self.busy.fetch_add(1, Ordering::SeqCst) >= self.made {
            self.made += 1;
            let (shared, threads, queue, busy) = (
                Arc::clone(&self.shared),
                Arc::clone(&self.threads),
                Arc::clone(&self.queue),
                Arc::clone(&self.busy),
            );
            let main = self.main.clone();
            let body = move || {
                let mut host = threads();
                loop {
                    let next = lock(&queue).recv();
                    let Ok((op, req)) = next else {
                        return;
                    };
                    operation(&shared, &mut *host, op, &req);
                    busy.fetch_sub(1, Ordering::SeqCst);
                }
            };
            let report = move |why| {
                let _ = main.send(ToMain::Closed(why));
            };
            watch::spawn(format!("qap-worker-{}", self.made), &self.watch, body, report);
        }
        let _ = self.work.send((op, req));
    }
}

/// Answer one operation on a secondary thread.
fn operation(shared: &Shared, host: &mut dyn ThreadHost, op: Op, req: &Envelope) {
    if shared.cancelled_before(req) {
        return;
    }
    let a = match op {
        Op::SignOn => sign_on(shared, host, req),
        Op::ChangePassword => change_password(shared, host, req),
        Op::Resume => resume(shared, host, req),
        Op::SignOff => sign_off(shared, host),
        Op::SystemInfo => Answer::ok(&host.system_info()),
        Op::PasswordStatus => Answer::ok(&host.password_status()),
        Op::TelnetTicket => match host.telnet_ticket() {
            Ok(t) => Answer::ok(&t),
            Err(r) => Answer::refused(r),
        },
        Op::ListSpooledFiles => body(req, |b: ListSpooledFiles| ok(host.list_spooled_files(&b))),
        Op::ReadSpooledFile => body(req, |b: ReadSpooledFile| ok(host.read_spooled_file(&b))),
        Op::HoldSpooledFile => body(req, |b: HoldSpooledFile| done(host.hold_spooled_file(&b.id, b.immediately))),
        Op::ReleaseSpooledFile => body(req, |b: SpooledFileRef| done(host.release_spooled_file(&b.id))),
        Op::DeleteSpooledFile => body(req, |b: SpooledFileRef| done(host.delete_spooled_file(&b.id))),
        Op::MoveSpooledFile => body(req, |b: MoveSpooledFile| done(host.move_spooled_file(&b.id, &b.outq, b.first))),
        Op::ListOutputQueues => body(req, |b: ListOutputQueues| ok(host.list_output_queues(&b))),
        _ => Answer::status(Status::InvalidRequest),
    };
    shared.finish(req, a);
}

/// The request's body decoded and answered; `InvalidRequest` when it
/// does not decode.
fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

/// A reply body, or the refusal.
fn ok(r: Result<impl Wire, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

/// Done with no body, or the refusal.
fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// A reply that opens or resumes a session.
fn opened(shared: &Shared, r: Result<SignOnReply, Refusal>) -> Answer {
    match r {
        Ok(reply) => {
            *lock(&shared.session) = Some((reply.token.clone(), reply.profile.clone()));
            *lock(&shared.watch.profile) = Some(reply.profile.clone());
            shared.swapped();
            Answer::ok(&reply)
        }
        Err(r) => Answer::refused(r),
    }
}

fn sign_on(shared: &Shared, host: &mut dyn ThreadHost, req: &Envelope) -> Answer {
    let Ok(b) = req.body_as::<SignOn>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let _one = lock(&shared.signon);
    if shared.signed_on() {
        return Answer::status(Status::InvalidRequest);
    }
    let r = host.sign_on(&b.user, &b.password);
    opened(shared, r)
}

fn change_password(shared: &Shared, host: &mut dyn ThreadHost, req: &Envelope) -> Answer {
    let Ok(b) = req.body_as::<ChangePassword>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let _one = lock(&shared.signon);
    let r = host.change_password(&b.user, &b.old_password, &b.new_password);
    let token = lock(&shared.session).as_ref().map(|(t, _)| t.clone());
    match (token, r) {
        // Signed on: the session and its token go on.
        (Some(token), Ok(mut reply)) => {
            reply.token = token;
            Answer::ok(&reply)
        }
        (_, r) => opened(shared, r),
    }
}

fn resume(shared: &Shared, host: &mut dyn ThreadHost, req: &Envelope) -> Answer {
    let Ok(b) = req.body_as::<Resume>() else {
        return Answer::status(Status::InvalidRequest);
    };
    let _one = lock(&shared.signon);
    if shared.signed_on() {
        return Answer::status(Status::InvalidRequest);
    }
    let r = host.resume(&b.token);
    opened(shared, r)
}

fn sign_off(shared: &Shared, host: &mut dyn ThreadHost) -> Answer {
    let _one = lock(&shared.signon);
    let session = lock(&shared.session).take();
    *lock(&shared.watch.profile) = None;
    if let Some((token, _)) = session {
        host.sign_off(&token);
        shared.swapped();
    }
    Answer::status(Status::Ok)
}
