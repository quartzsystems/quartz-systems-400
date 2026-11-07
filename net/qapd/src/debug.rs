// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Debug` service (version 9, draft; docs/QAP.md "Service 11
//! `Debug`"): each request decoded and handed to the host's
//! [`DebugHost`], on a secondary thread like every operation outside
//! the lane. A connection has at most one debug session. Its events,
//! `Stopped` and `DebugEnded`, are the host's news, taken by the
//! watches' thread at each look and sent from there, as the watches'
//! events are. The session ends with `End`, with a sign-off on the
//! connection (`DebugEnded` with [`SIGNED_OFF`]) and with the
//! connection.

use ipc::Wire;
use qap_proto::ops::debug::{
    AddProgram, DebugEnded, Eval, RemoveServiceEntry, Resume, SetBreakpoints, SetServiceEntry,
    Start, Started, Stopped, Variables,
};
use qap_proto::ops::event;
use qap_proto::{Envelope, Op, ServerMessage, Service, Status};

use crate::conn::{Answer, Shared, lock};
use crate::host::{Refusal, ThreadHost};

/// The message of a host without a debugger.
pub const NO_DEBUG: &str = "TCP1A70";
/// No debug session is active on the connection.
pub const NO_SESSION: &str = "TCP1A71";
/// A debug session is already active on the connection (`InUse`).
pub const SESSION_ACTIVE: &str = "TCP1A72";
/// No service entry point is set for the program (`NotFound`).
pub const NO_ENTRY: &str = "TCP1A73";
/// Another user's jobs need `*SERVICE` (`NotAuthorized`).
pub const OTHER_USER: &str = "TCP1A74";
/// `DebugEnded`: the debugged job ended.
pub const JOB_ENDED: &str = "TCP1A75";
/// `DebugEnded`: the connection signed off.
pub const SIGNED_OFF: &str = "TCP1A76";

/// What the host has to tell the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum News {
    /// A debugged job stopped; it waits for `Resume`.
    Stopped(Stopped),
    /// The session ended without `End`: why.
    Ended(Option<ServerMessage>),
}

/// The debugger a connection's secondary threads reach: the job's own
/// calls (`Own`) or another job's, as `STRSRVJOB` reaches it. Every
/// method but [`DebugHost::start`] answers `Error` with [`NO_SESSION`]
/// when no session is active.
pub trait DebugHost {
    /// Start the connection's session; its number. `InUse` with
    /// [`SESSION_ACTIVE`] when one is active.
    fn start(&mut self, req: &Start) -> Result<Started, Refusal>;
    /// End it, letting a stopped job go on; nothing when none is active.
    fn end(&mut self);
    /// Whether a session is active.
    fn active(&mut self) -> bool;
    /// Put a program under debug: its modules and their views.
    fn add_program(
        &mut self,
        req: &AddProgram,
    ) -> Result<qap_proto::ops::debug::ProgramAdded, Refusal>;
    /// A module's breakpoints from now.
    fn set_breakpoints(
        &mut self,
        req: &SetBreakpoints,
    ) -> Result<qap_proto::ops::debug::BreakpointsSet, Refusal>;
    /// Let the stopped job go on.
    fn resume(&mut self, req: &Resume) -> Result<(), Refusal>;
    /// The stopped job's call stack.
    fn stack(&mut self) -> Result<qap_proto::ops::debug::Frames, Refusal>;
    /// A frame's variables.
    fn variables(
        &mut self,
        req: &Variables,
    ) -> Result<qap_proto::ops::debug::VariableList, Refusal>;
    /// `EVAL` at the stop.
    fn eval(&mut self, req: &Eval) -> Result<qap_proto::ops::debug::Evaluated, Refusal>;
    /// Stop the jobs of a user when they call a program.
    fn set_service_entry(&mut self, req: &SetServiceEntry) -> Result<(), Refusal>;
    /// Remove a service entry point.
    fn remove_service_entry(&mut self, req: &RemoveServiceEntry) -> Result<(), Refusal>;
    /// What happened since the last call, oldest first.
    fn news(&mut self) -> Vec<News>;
}

/// The request's body decoded and answered.
fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

fn ok(r: Result<impl Wire, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// Whether the watches' thread looks for the session's news.
fn set_looking(shared: &Shared, on: bool) {
    lock(&shared.watches).debugging = on;
}

/// Answer one `Debug` operation.
pub(crate) fn serve(
    shared: &std::sync::Arc<Shared>,
    host: &mut dyn ThreadHost,
    op: Op,
    req: &Envelope,
) -> Answer {
    if host.debugger().is_none() {
        return Answer::refused(Refusal {
            status: Status::Error,
            message: host.message(NO_DEBUG, &[]),
        });
    }
    let Some(d) = host.debugger() else {
        return Answer::status(Status::Error);
    };
    match op {
        Op::DebugStart => body(req, |b: Start| {
            let r = d.start(&b);
            if r.is_ok() {
                set_looking(shared, true);
                crate::watches::start(shared);
            }
            ok(r)
        }),
        Op::DebugEnd => {
            if !d.active() {
                return no_session(host);
            }
            d.end();
            set_looking(shared, false);
            Answer::status(Status::Ok)
        }
        Op::AddProgram => body(req, |b: AddProgram| ok(d.add_program(&b))),
        Op::SetBreakpoints => body(req, |b: SetBreakpoints| {
            if b.lines.iter().any(|l| l.line == 0) {
                return Answer::status(Status::InvalidRequest);
            }
            ok(d.set_breakpoints(&b))
        }),
        Op::DebugResume => body(req, |b: Resume| done(d.resume(&b))),
        Op::Stack => ok(d.stack()),
        Op::Variables => body(req, |b: Variables| ok(d.variables(&b))),
        Op::Eval => body(req, |b: Eval| ok(d.eval(&b))),
        Op::SetServiceEntry => body(req, |b: SetServiceEntry| done(d.set_service_entry(&b))),
        Op::RemoveServiceEntry => body(req, |b: RemoveServiceEntry| {
            done(d.remove_service_entry(&b))
        }),
        _ => Answer::status(Status::InvalidRequest),
    }
}

fn no_session(host: &mut dyn ThreadHost) -> Answer {
    Answer::refused(Refusal {
        status: Status::Error,
        message: host.message(NO_SESSION, &[]),
    })
}

/// The watches' thread: send the session's news as events.
pub(crate) fn look(shared: &Shared, host: &mut dyn ThreadHost) {
    if !lock(&shared.watches).debugging {
        return;
    }
    let Some(d) = host.debugger() else {
        return;
    };
    for n in d.news() {
        let e = match n {
            News::Stopped(s) => {
                Envelope::event(Service::Debug, event::STOPPED, Status::Ok, None).with_body(&s)
            }
            News::Ended(message) => {
                set_looking(shared, false);
                Envelope::event(Service::Debug, event::DEBUG_ENDED, Status::Ok, None)
                    .with_body(&DebugEnded { message })
            }
        };
        shared.send(vec![e]);
    }
}

/// A sign-off on the connection ends its session: `DebugEnded` with
/// [`SIGNED_OFF`].
pub(crate) fn signed_off(shared: &Shared, host: &mut dyn ThreadHost) {
    let active = host.debugger().is_some_and(DebugHost::active);
    if !active {
        return;
    }
    let m = host.message(SIGNED_OFF, &[]);
    if let Some(d) = host.debugger() {
        d.end();
    }
    set_looking(shared, false);
    shared.send(vec![
        Envelope::event(Service::Debug, event::DEBUG_ENDED, Status::Ok, None)
            .with_body(&DebugEnded { message: Some(m) }),
    ]);
}

/// The connection closes: a stopped job goes on.
pub(crate) fn closed(shared: &Shared, host: &mut dyn ThreadHost) {
    if let Some(d) = host.debugger() {
        d.end();
    }
    set_looking(shared, false);
}
