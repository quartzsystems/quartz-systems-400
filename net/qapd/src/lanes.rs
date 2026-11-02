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
use std::thread;

use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod, choose_version};
use qap_proto::ops::signon::{ChangePassword, Resume, SignOn, SignOnReply};
use qap_proto::{Envelope, HELLO_ID, Op, Service, Status, VERSION};

use crate::Closed;
use crate::conn::{Answer, Shared, lock};
use crate::host::{Refusal, ThreadHost, Threads};
use crate::pump::In;

/// From the dispatcher to the job's own thread.
pub(crate) enum ToMain {
    /// A `Command` request, in arrival order.
    Command(Op, Envelope),
    /// The connection is to close.
    Closed(Closed),
}

/// Route requests until the connection closes or `Hello` refuses it.
pub(crate) fn dispatch(shared: &Arc<Shared>, rx: &Receiver<In>, threads: &Threads, main: &Sender<ToMain>) {
    let mut own = threads();
    let mut pool = Pool::new(Arc::clone(shared), Arc::clone(threads));
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
    let _ = main.send(ToMain::Closed(Closed::Broken));
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
}

impl Pool {
    fn new(shared: Arc<Shared>, threads: Threads) -> Self {
        let (work, queue) = mpsc::channel();
        Self {
            shared,
            threads,
            work,
            queue: Arc::new(Mutex::new(queue)),
            made: 0,
            busy: Arc::new(AtomicUsize::new(0)),
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
            thread::spawn(move || {
                let mut host = threads();
                loop {
                    let next = lock(&queue).recv();
                    let Ok((op, req)) = next else {
                        return;
                    };
                    operation(&shared, &mut *host, op, &req);
                    busy.fetch_sub(1, Ordering::SeqCst);
                }
            });
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
        _ => Answer::status(Status::InvalidRequest),
    };
    shared.finish(req, a);
}

/// A reply that opens or resumes a session.
fn opened(shared: &Shared, r: Result<SignOnReply, Refusal>) -> Answer {
    match r {
        Ok(reply) => {
            *lock(&shared.session) = Some((reply.token.clone(), reply.profile.clone()));
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
    if let Some((token, _)) = session {
        host.sign_off(&token);
        shared.swapped();
    }
    Answer::status(Status::Ok)
}
