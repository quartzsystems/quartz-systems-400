// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection job's own thread: the command lane, where `Command`
//! requests run one at a time in arrival order (they share the job's
//! state), and the events when the server or an operator ends the
//! connection. Every other request runs in [`crate::lanes`] meanwhile.
//! What the threads share: the version, the session, the replies
//! (only the version's values, split to fit a frame, `Cancelled` when
//! the client cancelled: empty before the operation started, its own
//! reply after).

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use ipc::Wire;
use qap_proto::ops::command::{CheckReply, CommandText, DescribeCommand};
use qap_proto::ops::event;
use qap_proto::{Envelope, Op, ServerMessage, Service, Status, Token};

use crate::host::{Ending, Host, Refusal};
use crate::lanes::{self, ToMain};
use crate::pump::{self, Cancels, In, Out, Running, cancelled};
use crate::{Closed, Config, Transport};

/// A lock, poison or not.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An operation's answer.
pub(crate) struct Answer {
    pub status: Status,
    pub message: Option<ServerMessage>,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn ok(body: &impl Wire) -> Self {
        Self {
            status: Status::Ok,
            message: None,
            body: ipc::encode(body),
        }
    }

    pub fn status(status: Status) -> Self {
        Self {
            status,
            message: None,
            body: Vec::new(),
        }
    }

    pub fn refused(r: Refusal) -> Self {
        Self {
            status: r.status,
            message: Some(r.message),
            body: Vec::new(),
        }
    }
}

/// What the connection's threads share.
pub(crate) struct Shared {
    out: Mutex<Sender<Out>>,
    pub cancels: Cancels,
    /// The version `Hello` agreed; 0 before it.
    version: AtomicU16,
    /// The signed-on session's token and profile.
    pub session: Mutex<Option<(Token, String)>>,
    /// Bumped each time the job runs as another profile.
    pub swaps: AtomicU64,
    /// Sign-on, password changes, resume and sign-off, one at a time.
    pub signon: Mutex<()>,
}

impl Shared {
    pub fn version(&self) -> Option<u16> {
        match self.version.load(Ordering::SeqCst) {
            0 => None,
            v => Some(v),
        }
    }

    pub fn set_version(&self, v: u16) {
        self.version.store(v, Ordering::SeqCst);
    }

    pub fn signed_on(&self) -> bool {
        lock(&self.session).is_some()
    }

    pub fn send(&self, mut frames: Vec<Envelope>) {
        // A peer gets only the statuses its version defines.
        let v = self.version().unwrap_or(qap_proto::MIN_VERSION);
        for f in &mut frames {
            f.status = f.status.for_version(v);
        }
        let _ = lock(&self.out).send(Out::Frames(frames));
    }

    pub fn reply(&self, req: &Envelope, a: Answer) {
        match Envelope::replies(req, a.status, a.message, a.body) {
            Ok(frames) => self.send(frames),
            Err(_) => self.send(vec![Envelope::reply(req, Status::Error, None, Vec::new())]),
        }
    }

    /// Whether the client cancelled `req`; if so, its empty reply is
    /// sent. Asked before an operation starts.
    pub fn cancelled_before(&self, req: &Envelope) -> bool {
        let c = cancelled(&self.cancels, req.request_id);
        if c {
            self.reply(req, Answer::status(Status::Cancelled));
        }
        c
    }

    /// Reply with an operation's answer; `Cancelled` with the
    /// operation's own body and message when cancelled once started.
    pub fn finish(&self, req: &Envelope, a: Answer) {
        let a = if cancelled(&self.cancels, req.request_id) {
            Answer {
                status: Status::Cancelled,
                ..a
            }
        } else {
            a
        };
        self.reply(req, a);
    }

    /// The job now runs as another profile.
    pub fn swapped(&self) {
        self.swaps.fetch_add(1, Ordering::SeqCst);
    }

    fn close(&self) {
        let _ = lock(&self.out).send(Out::Close);
    }
}

/// The command lane and the job's end.
struct Main<'h, H: Host> {
    host: &'h mut H,
    shared: Arc<Shared>,
    running: Arc<Running>,
    /// The swaps the job's runtime has followed.
    seen: u64,
}

impl<H: Host> Main<'_, H> {
    /// Run one `Command` request.
    fn command(&mut self, op: Op, req: &Envelope) {
        if self.shared.cancelled_before(req) {
            return;
        }
        let swaps = self.shared.swaps.load(Ordering::SeqCst);
        if swaps != self.seen {
            self.seen = swaps;
            self.host.swapped();
        }
        *lock(&self.running.id) = Some(req.request_id);
        let a = match op {
            Op::RunCommand => self.run_command(req),
            Op::CheckCommand => self.check_command(req),
            Op::DescribeCommand => match req.body_as::<DescribeCommand>() {
                Ok(b) => match self.host.describe_command(&b.command) {
                    Ok(d) => Answer::ok(&d),
                    Err(r) => Answer::refused(r),
                },
                Err(_) => Answer::status(Status::InvalidRequest),
            },
            _ => Answer::status(Status::InvalidRequest),
        };
        *lock(&self.running.id) = None;
        self.shared.finish(req, a);
    }

    fn run_command(&mut self, req: &Envelope) -> Answer {
        let Ok(b) = req.body_as::<CommandText>() else {
            return Answer::status(Status::InvalidRequest);
        };
        let ran = self.host.run_command(&b.command);
        Answer {
            status: ran.status,
            message: ran.escape,
            body: ipc::encode(&ran.reply),
        }
    }

    fn check_command(&mut self, req: &Envelope) -> Answer {
        let Ok(b) = req.body_as::<CommandText>() else {
            return Answer::status(Status::InvalidRequest);
        };
        let (status, messages) = self.host.check_command(&b.command);
        Answer {
            status,
            message: None,
            body: ipc::encode(&CheckReply { messages }),
        }
    }

    /// The server or an operator ends the connection.
    fn end(&mut self, why: Ending) -> Closed {
        match why {
            Ending::Server => {
                let m = self.host.message("TCP1A0A", &["QAP"]);
                self.shared.send(vec![Envelope::event(
                    Service::Connection,
                    event::SERVER_ENDING,
                    Status::ServerEnding,
                    Some(m),
                )]);
                Closed::ServerEnding
            }
            Ending::Operator => {
                let session = lock(&self.shared.session).take();
                if let Some((token, profile)) = session {
                    let m = self.host.message("TCP1A54", &[&profile]);
                    self.host.sign_off(&token);
                    self.shared.send(vec![Envelope::event(
                        Service::SignOn,
                        event::SESSION_ENDED,
                        Status::Ok,
                        Some(m),
                    )]);
                }
                Closed::Operator
            }
        }
    }
}

/// Serve one connection until it closes; why it closed.
pub fn serve<T: Transport, H: Host>(t: T, host: &mut H, cfg: Config) -> Closed {
    let (in_tx, in_rx) = mpsc::channel::<In>();
    let (out_tx, out_rx) = mpsc::channel::<Out>();
    let (main_tx, main_rx) = mpsc::channel::<ToMain>();
    let shared = Arc::new(Shared {
        out: Mutex::new(out_tx),
        cancels: Cancels::default(),
        version: AtomicU16::new(0),
        session: Mutex::new(None),
        swaps: AtomicU64::new(0),
        signon: Mutex::new(()),
    });
    let running = Arc::new(Running {
        id: Mutex::new(None),
        cancel: host.canceller(),
    });
    let pump_cancels = Cancels::clone(&shared.cancels);
    let pump_running = Arc::clone(&running);
    let pump = thread::spawn(move || pump::run(t, cfg, in_tx, &out_rx, pump_cancels, pump_running));
    let threads = host.threads();
    let lanes_shared = Arc::clone(&shared);
    let dispatch = thread::spawn(move || lanes::dispatch(&lanes_shared, &in_rx, &threads, &main_tx));
    let mut m = Main {
        host,
        shared: Arc::clone(&shared),
        running,
        seen: 0,
    };
    let why = loop {
        match main_rx.recv_timeout(cfg.poll) {
            Ok(ToMain::Command(op, req)) => m.command(op, &req),
            Ok(ToMain::Closed(why)) => break why,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Closed::Broken,
        }
        if let Some(e) = m.host.ending() {
            break m.end(e);
        }
    };
    // A session outlives its connection until its idle timeout.
    let session = lock(&shared.session).take();
    if let Some((token, _)) = session {
        m.host.detach(&token);
    }
    shared.close();
    let _ = pump.join();
    let _ = dispatch.join();
    why
}
