// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 9 (draft): the `Debug` service against the test system: an
//! `Own` session stopping the connection's `CALL`, breakpoints by line,
//! variables and `EVAL`, steps, a service entry point attaching a
//! display session's job, another job by `Start`, the statuses and
//! messages, the session's ends, and older versions without it.

#![allow(clippy::many_single_char_names)] // c, d, s, v, e: the system, the client, what came

mod common;

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use ipc::Wire;
use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::debug::{
    AddProgram, BreakpointsSet, DebugEnded, DebugTarget, Eval, Evaluated, Frames, LineBreakpoint,
    ProgramAdded, RemoveServiceEntry, Resume, ResumeHow, SetBreakpoints, SetServiceEntry, Start,
    Started, StopReason, Stopped, VariableList, Variables, View,
};
use qap_proto::ops::event;
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Kind, Op, Service, Status};
use qapd::Config;

use common::{Controls, Run, start_with};

fn quick() -> Config {
    Config {
        poll: Duration::from_millis(10),
        watch_poll: Duration::from_millis(20),
        ..Config::default()
    }
}

/// A client that keeps what comes while it waits for something else.
struct Dbg {
    r: Run,
    held: VecDeque<Envelope>,
}

impl Dbg {
    fn new(c: &Arc<Controls>, user: &str, password: &str) -> Self {
        let mut r = start_with(quick(), Arc::clone(c));
        let h: HelloReply = r.client.hello_range(1, 9).body_as().unwrap();
        assert_eq!(h.version, 9);
        assert!(h.services.contains(&Service::Debug));
        r.client.sign_on(user, password);
        Self {
            r,
            held: VecDeque::new(),
        }
    }

    fn reply(&mut self, id: u32) -> Envelope {
        if let Some(i) = self
            .held
            .iter()
            .position(|e| e.kind == Kind::Reply && e.request_id == id)
        {
            return self.held.remove(i).unwrap();
        }
        loop {
            let e = self.r.client.recv().expect("a reply");
            if e.kind == Kind::Reply && e.request_id == id {
                return e;
            }
            self.held.push_back(e);
        }
    }

    fn call<B: Wire>(&mut self, op: Op, body: &B) -> Envelope {
        let id = self.r.client.request(op, body);
        self.reply(id)
    }

    fn ok<B: Wire, R: Wire>(&mut self, op: Op, body: &B) -> R {
        let e = self.call(op, body);
        assert_eq!(e.status, Status::Ok, "{op:?}: {:?}", e.message);
        e.body_as().unwrap()
    }

    fn refused<B: Wire>(&mut self, op: Op, body: &B, status: Status, id: &str) {
        let e = self.call(op, body);
        assert_eq!(e.status, status, "{op:?}: {:?}", e.message);
        assert_eq!(
            e.message.as_ref().map(|m| m.id.as_str()),
            Some(id),
            "{op:?}"
        );
    }

    fn event(&mut self) -> Envelope {
        if let Some(i) = self.held.iter().position(|e| e.kind == Kind::Event) {
            return self.held.remove(i).unwrap();
        }
        loop {
            let e = self.r.client.recv().expect("an event");
            if e.kind == Kind::Event {
                return e;
            }
            self.held.push_back(e);
        }
    }

    fn stopped(&mut self) -> Stopped {
        let e = self.event();
        assert_eq!((e.service, e.op), (Service::Debug, event::STOPPED), "{e:?}");
        e.body_as().unwrap()
    }

    fn ended(&mut self) -> DebugEnded {
        let e = self.event();
        assert_eq!(
            (e.service, e.op),
            (Service::Debug, event::DEBUG_ENDED),
            "{e:?}"
        );
        e.body_as().unwrap()
    }

    fn resume(&mut self, how: ResumeHow) {
        let e = self.call(Op::DebugResume, &Resume { how });
        assert_eq!(e.status, Status::Ok, "{:?}", e.message);
    }

    fn start(&mut self, target: DebugTarget) -> Started {
        self.ok(Op::DebugStart, &Start { target })
    }

    fn breakpoints(&mut self, lines: &[(u32, Option<&str>)]) -> Envelope {
        self.call(
            Op::SetBreakpoints,
            &SetBreakpoints {
                program: assets(),
                module: "ASSETS".into(),
                lines: lines
                    .iter()
                    .map(|(line, c)| LineBreakpoint {
                        line: *line,
                        condition: c.map(String::from),
                    })
                    .collect(),
            },
        )
    }
}

fn assets() -> ObjectName {
    ObjectName {
        library: "HLINV".into(),
        name: "ASSETS".into(),
    }
}

fn add(d: &mut Dbg, name: &str) -> Envelope {
    d.call(
        Op::AddProgram,
        &AddProgram {
            program: ObjectName {
                library: "HLINV".into(),
                name: name.into(),
            },
            r#type: "*PGM".into(),
        },
    )
}

fn call_assets(d: &mut Dbg) -> u32 {
    d.r.client.request(
        Op::RunCommand,
        &CommandText {
            command: "CALL PGM(HLINV/ASSETS)".into(),
        },
    )
}

#[test]
fn own_session_stops_the_connections_call() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    assert!(d.start(DebugTarget::Own).session > 0);
    let added: ProgramAdded = add(&mut d, "ASSETS").body_as().unwrap();
    assert_eq!(added.modules[0].name, "ASSETS");
    assert_eq!(added.modules[0].views, [View::Statement, View::Source]);
    // Line 11 is a comment: the breakpoint lands on 12.
    let set: BreakpointsSet = d.breakpoints(&[(11, None)]).body_as().unwrap();
    assert_eq!((set.set[0].line, set.set[0].stmt), (12, 12));
    let call = call_assets(&mut d);
    let s = d.stopped();
    assert_eq!(s.reason, StopReason::Breakpoint);
    assert_eq!((s.frame.line, s.frame.view), (12, View::Source));
    assert_eq!(s.frame.program, assets());
    assert_eq!(s.job.name, "QAPCNN");
    // The command waits while the program is stopped.
    assert!(d.r.client.try_recv(Duration::from_millis(100)).is_none());
    let vars: VariableList = d.ok(
        Op::Variables,
        &Variables {
            frame: 0,
            parent: None,
        },
    );
    let names: Vec<(&str, &str, bool)> = vars
        .variables
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str(), v.expandable))
        .collect();
    assert_eq!(
        names,
        [
            ("COUNT", "1", false),
            ("NAME", "'DESK      '", false),
            ("ASSET", "", true)
        ]
    );
    let parts: VariableList = d.ok(
        Op::Variables,
        &Variables {
            frame: 0,
            parent: Some("ASSET".into()),
        },
    );
    assert_eq!(parts.variables.len(), 2);
    assert_eq!(parts.variables[0].name, "ASSET.ID");
    let v: Evaluated = d.ok(
        Op::Eval,
        &Eval {
            frame: 0,
            expression: "count".into(),
        },
    );
    assert_eq!(v.value, "1");
    d.refused(
        Op::Eval,
        &Eval {
            frame: 0,
            expression: "NOPE".into(),
        },
        Status::Error,
        "CPF7E12",
    );
    let stack: Frames = d.ok(Op::Stack, &());
    assert_eq!(stack.frames[0].line, 12);
    // A step: the next statement.
    d.resume(ResumeHow::StepOver);
    let s = d.stopped();
    assert_eq!((s.reason, s.frame.line), (StopReason::Step, 13));
    d.resume(ResumeHow::Go);
    let r = d.reply(call);
    assert_eq!(r.status, Status::Ok, "{:?}", r.message);
    // Nothing is stopped now.
    d.refused(
        Op::DebugResume,
        &Resume { how: ResumeHow::Go },
        Status::Error,
        "CPD9CCC",
    );
    d.refused(Op::Stack, &(), Status::Error, "CPD9CCC");
    assert_eq!(d.call(Op::DebugEnd, &()).status, Status::Ok);
    d.refused(Op::DebugEnd, &(), Status::Error, "TCP1A71");
    d.refused(Op::Stack, &(), Status::Error, "TCP1A71");
    // Without a session the program runs through.
    let call = call_assets(&mut d);
    assert_eq!(d.reply(call).status, Status::Ok);
}

#[test]
fn breakpoints_and_programs_refused() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    d.refused(
        Op::AddProgram,
        &AddProgram::default(),
        Status::Error,
        "TCP1A71",
    );
    d.start(DebugTarget::Own);
    d.refused(
        Op::DebugStart,
        &Start {
            target: DebugTarget::Own,
        },
        Status::InUse,
        "TCP1A72",
    );
    let e = add(&mut d, "NODEBUG");
    assert_eq!(e.status, Status::Error);
    assert_eq!(e.message.unwrap().id, qapd::testing::debug::NO_DEBUG_DATA);
    let e = add(&mut d, "NOSUCH");
    assert_eq!(
        (e.status, e.message.unwrap().id.as_str()),
        (Status::NotFound, "CPF9801")
    );
    add(&mut d, "ASSETS");
    let e = d.breakpoints(&[(15, None)]);
    assert_eq!(
        (e.status, e.message.unwrap().id.as_str()),
        (Status::Error, "CPD9CCB")
    );
    let e = d.breakpoints(&[(9, Some("COUNT ~ 1"))]);
    assert_eq!(
        (e.status, e.message.unwrap().id.as_str()),
        (Status::Error, "CPF9C59")
    );
    assert_eq!(d.breakpoints(&[(0, None)]).status, Status::InvalidRequest);
    // A condition that never holds: the program runs through.
    assert_eq!(d.breakpoints(&[(10, Some("COUNT = 9"))]).status, Status::Ok);
    let call = call_assets(&mut d);
    assert_eq!(d.reply(call).status, Status::Ok);
    // One that holds stops it.
    d.breakpoints(&[(10, Some("COUNT = 1"))]);
    let call = call_assets(&mut d);
    assert_eq!(d.stopped().frame.line, 10);
    d.refused(
        Op::Variables,
        &Variables {
            frame: 3,
            parent: None,
        },
        Status::InvalidRequest,
        "TCP1A77",
    );
    // Breakpoints cleared while stopped: Go runs to the end.
    let none: BreakpointsSet = d.breakpoints(&[]).body_as().unwrap();
    assert!(none.set.is_empty());
    d.resume(ResumeHow::Go);
    assert_eq!(d.reply(call).status, Status::Ok);
}

#[test]
fn a_cancel_while_stopped_ends_the_command() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    d.start(DebugTarget::Own);
    add(&mut d, "ASSETS");
    d.breakpoints(&[(9, None)]);
    let call = call_assets(&mut d);
    d.stopped();
    d.r.client.send(&Envelope::cancel(call));
    let r = d.reply(call);
    assert_eq!(r.status, Status::Cancelled);
}

#[test]
fn a_service_entry_point_attaches_a_display_job() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    // Not without a session.
    let entry = SetServiceEntry {
        program: assets(),
        user: "*CURRENT".into(),
    };
    d.refused(Op::SetServiceEntry, &entry, Status::Error, "TCP1A71");
    d.start(DebugTarget::Own);
    assert_eq!(d.call(Op::SetServiceEntry, &entry).status, Status::Ok);
    d.refused(
        Op::SetServiceEntry,
        &SetServiceEntry {
            program: assets(),
            user: "ANN".into(),
        },
        Status::NotAuthorized,
        "TCP1A74",
    );
    // ANN's job calls it and runs on; BOB's stops at its first statement.
    let ann = c.interactive_job("ANN");
    c.call_in(&ann).join().unwrap();
    let job = c.interactive_job("BOB");
    let running = c.call_in(&job);
    let s = d.stopped();
    assert_eq!(s.reason, StopReason::ServiceEntry);
    assert_eq!(s.job, job);
    assert_eq!(s.frame.line, 9);
    // The display job waits.
    std::thread::sleep(Duration::from_millis(100));
    assert!(!running.is_finished());
    d.breakpoints(&[(13, None)]);
    d.resume(ResumeHow::Go);
    let s = d.stopped();
    assert_eq!((s.reason, s.frame.line), (StopReason::Breakpoint, 13));
    let v: Evaluated = d.ok(
        Op::Eval,
        &Eval {
            frame: 0,
            expression: "ASSET.TAG".into(),
        },
    );
    assert_eq!(v.value, "'A1      '");
    d.resume(ResumeHow::Go);
    running.join().unwrap();
    let rm = RemoveServiceEntry { program: assets() };
    assert_eq!(d.call(Op::RemoveServiceEntry, &rm).status, Status::Ok);
    d.refused(Op::RemoveServiceEntry, &rm, Status::NotFound, "TCP1A73");
}

#[test]
fn another_job_by_start() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    let ann = c.interactive_job("ANN");
    let job = |j: &JobId| DebugTarget::Job { job: j.clone() };
    d.refused(
        Op::DebugStart,
        &Start { target: job(&ann) },
        Status::NotAuthorized,
        "CPF3502",
    );
    let nobody = JobId {
        name: "NOSUCH".into(),
        user: String::new(),
        number: String::new(),
    };
    d.refused(
        Op::DebugStart,
        &Start {
            target: job(&nobody),
        },
        Status::NotFound,
        "CPF3520",
    );
    let mine = c.interactive_job("BOB");
    d.start(job(&mine));
    add(&mut d, "ASSETS");
    d.breakpoints(&[(14, None)]);
    let running = c.call_in(&mine);
    let s = d.stopped();
    assert_eq!((s.job.clone(), s.frame.line), (mine.clone(), 14));
    // A second connection may not take the same job.
    let mut other = Dbg::new(&c, "SECOFR", "SECPW01");
    other.refused(
        Op::DebugStart,
        &Start { target: job(&mine) },
        Status::InUse,
        "CPF3503",
    );
    // SECOFR has *SERVICE: ANN's job is allowed.
    other.start(job(&ann));
    d.resume(ResumeHow::Go);
    running.join().unwrap();
    // The job ends: the session ends with it.
    c.end_job(&mine);
    let e = d.ended();
    assert_eq!(e.message.unwrap().id, "TCP1A75");
    d.refused(Op::Stack, &(), Status::Error, "TCP1A71");
}

#[test]
fn sign_off_ends_the_session() {
    let c = Controls::new();
    let mut d = Dbg::new(&c, "BOB", "SECRET1");
    d.start(DebugTarget::Own);
    assert_eq!(d.call(Op::SignOff, &()).status, Status::Ok);
    let e = d.ended();
    assert_eq!(e.message.unwrap().id, "TCP1A76");
}

#[test]
fn a_version_8_peer_has_no_debugger() {
    let c = Controls::new();
    let mut r = start_with(quick(), Arc::clone(&c));
    let h: HelloReply = r.client.hello_range(1, 8).body_as().unwrap();
    assert_eq!(h.version, 8);
    assert!(!h.services.contains(&Service::Debug));
    r.client.sign_on("BOB", "SECRET1");
    let e = r.client.call(
        Op::DebugStart,
        &Start {
            target: DebugTarget::Own,
        },
    );
    assert_eq!(e.status, Status::InvalidRequest);
    // A display job's call meets no session of this connection.
    let job = c.interactive_job("BOB");
    c.call_in(&job).join().unwrap();
    assert!(r.client.try_recv(Duration::from_millis(200)).is_none());
}
