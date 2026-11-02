// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection's thread: requests in arrival order, `Hello` first
//! and once, the operations allowed before sign-on, replies split to
//! fit a frame, cancels honoured before and after an operation runs,
//! and the events when the server or an operator ends the connection.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;

use ipc::Wire;
use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod, choose_version};
use qap_proto::ops::event;
use qap_proto::{Envelope, HELLO_ID, Op, ServerMessage, Service, Status, VERSION};

use crate::host::{Ending, Host};
use crate::pump::{self, Cancels, In, Out, cancelled};
use crate::{Closed, Config, Transport};

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
}

/// What the connection knows.
struct Conn<'h, H: Host> {
    host: &'h mut H,
    out: Sender<Out>,
    cancels: Cancels,
    /// The version `Hello` agreed; `None` before it.
    version: Option<u16>,
}

impl<H: Host> Conn<'_, H> {
    fn send(&self, frames: Vec<Envelope>) {
        let _ = self.out.send(Out::Frames(frames));
    }

    fn reply(&self, req: &Envelope, a: Answer) {
        match Envelope::replies(req, a.status, a.message, a.body) {
            Ok(frames) => self.send(frames),
            Err(_) => self.send(vec![Envelope::reply(req, Status::Error, None, Vec::new())]),
        }
    }

    /// Answer one request; `Some` when the connection is to close.
    fn request(&mut self, req: &Envelope) -> Option<Closed> {
        if cancelled(&self.cancels, req.request_id) {
            self.reply(req, Answer::status(Status::Cancelled));
            return None;
        }
        let op = req.operation();
        if self.version.is_none() {
            // The first request is Hello, ID 1; anything else closes.
            if op != Some(Op::Hello) || req.request_id != HELLO_ID {
                self.reply(req, Answer::status(Status::InvalidRequest));
                return Some(Closed::Refused);
            }
            return self.hello(req);
        }
        let a = match op {
            None => Answer::status(Status::InvalidRequest),
            Some(Op::Hello) => {
                self.reply(req, Answer::status(Status::InvalidRequest));
                return Some(Closed::Refused);
            }
            Some(op) => self.operation(op, req),
        };
        let a = if cancelled(&self.cancels, req.request_id) {
            Answer::status(Status::Cancelled)
        } else {
            a
        };
        self.reply(req, a);
        None
    }

    fn hello(&mut self, req: &Envelope) -> Option<Closed> {
        let Ok(h) = req.body_as::<Hello>() else {
            self.reply(req, Answer::status(Status::InvalidRequest));
            return Some(Closed::Refused);
        };
        let Some(version) = choose_version(h.min_version, h.max_version, VERSION) else {
            self.reply(req, Answer::status(Status::VersionUnsupported));
            return Some(Closed::Refused);
        };
        self.version = Some(version);
        let id = self.host.system_id();
        let reply = HelloReply {
            version,
            system_name: id.system_name,
            release: id.release,
            services: Service::ALL.to_vec(),
            capabilities: Vec::new(),
            signon_methods: vec![SignOnMethod::Password],
            server_time: self.host.now_ms(),
            time_zone: id.time_zone,
            system_ccsid: id.ccsid,
        };
        self.reply(req, Answer::ok(&reply));
        None
    }

    fn operation(&mut self, op: Op, _req: &Envelope) -> Answer {
        match op {
            Op::Ping => Answer::ok(&PingReply {
                server_time: self.host.now_ms(),
            }),
            Op::SignOff | Op::SystemInfo => Answer::status(Status::NotSignedOn),
            Op::Hello | Op::SignOn | Op::ChangePassword | Op::Resume => Answer::status(Status::InvalidRequest),
        }
    }

    /// The server or an operator ends the connection.
    fn end(&mut self, why: Ending) -> Closed {
        match why {
            Ending::Server => {
                let m = self.host.message("TCP1A0A", &["QAP"]);
                self.send(vec![Envelope::event(
                    Service::Connection,
                    event::SERVER_ENDING,
                    Status::ServerEnding,
                    Some(m),
                )]);
                Closed::ServerEnding
            }
            Ending::Operator => Closed::Operator,
        }
    }
}

/// Serve one connection until it closes; why it closed.
pub fn serve<T: Transport, H: Host>(t: T, host: &mut H, cfg: Config) -> Closed {
    let (in_tx, in_rx) = mpsc::channel::<In>();
    let (out_tx, out_rx) = mpsc::channel::<Out>();
    let cancels = Cancels::default();
    let pump_cancels = Cancels::clone(&cancels);
    let pump = thread::spawn(move || pump::run(t, cfg, in_tx, &out_rx, pump_cancels));
    let mut c = Conn {
        host,
        out: out_tx,
        cancels,
        version: None,
    };
    let why = loop {
        match in_rx.recv_timeout(cfg.poll) {
            Ok(In::Request(req)) => {
                if let Some(why) = c.request(&req) {
                    break why;
                }
            }
            Ok(In::Closed(why)) => break why,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break Closed::Broken,
        }
        if let Some(e) = c.host.ending() {
            break c.end(e);
        }
    };
    let _ = c.out.send(Out::Close);
    let _ = pump.join();
    why
}
