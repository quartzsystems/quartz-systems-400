// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 9 (draft): the `Debug` service's bodies, values and gates.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::debug::{
    AddProgram, BreakpointSet, BreakpointsSet, DebugEnded, DebugModule, DebugTarget, Eval,
    Evaluated, Frame, Frames, LineBreakpoint, ProgramAdded, RemoveServiceEntry, Resume, ResumeHow,
    SetBreakpoints, SetServiceEntry, Start, Started, StopReason, Stopped, Variable, VariableList,
    Variables, View,
};
use qap_proto::ops::event;
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, ServerMessage, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn assets() -> ObjectName {
    ObjectName {
        library: "HLINV".into(),
        name: "ASSETS".into(),
    }
}

fn job() -> JobId {
    JobId {
        name: "QPADEV0001".into(),
        user: "BOB".into(),
        number: "004711".into(),
    }
}

fn frame() -> Frame {
    Frame {
        program: assets(),
        module: "ASSETS".into(),
        procedure: "ASSETS".into(),
        stmt: 42,
        line: 42,
        view: View::Source,
    }
}

#[test]
fn version_9_values() {
    const { assert!(VERSION >= 9) };
    assert_eq!(encode(&Service::Debug), [11]);
    assert_eq!(Service::Debug.since(), 9);
    let ops = [
        Op::DebugStart,
        Op::DebugEnd,
        Op::AddProgram,
        Op::SetBreakpoints,
        Op::DebugResume,
        Op::Stack,
        Op::Variables,
        Op::Eval,
        Op::SetServiceEntry,
        Op::RemoveServiceEntry,
    ];
    for (code, op) in (1..=10).zip(ops) {
        assert_eq!(Op::of(Service::Debug, code), Some(op));
        assert_eq!(op.code(), (Service::Debug, code));
        assert_eq!(op.since(), 9, "{op:?}");
        assert!(!op.before_signon() && !op.in_lane(), "{op:?}");
    }
    assert_eq!(Op::of(Service::Debug, 11), None);
    for v in 1..=8 {
        assert!(!Service::of_version(v).contains(&Service::Debug));
    }
    assert!(Service::of_version(9).contains(&Service::Debug));
    // No status of its own.
    assert_eq!(Status::ALL.len(), 16);
    assert_eq!(event::since(Service::Debug, event::STOPPED), Some(9));
    assert_eq!(event::since(Service::Debug, event::DEBUG_ENDED), Some(9));
    assert_eq!(event::since(Service::Debug, 102), None);
}

#[test]
fn a_version_8_peer_never_gets_a_debug_body() {
    let stopped =
        Envelope::event(Service::Debug, event::STOPPED, Status::Ok, None).with_body(&Stopped {
            job: job(),
            reason: StopReason::Breakpoint,
            frame: frame(),
        });
    let ended = Envelope::event(Service::Debug, event::DEBUG_ENDED, Status::Ok, None)
        .with_body(&DebugEnded { message: None });
    for e in [&stopped, &ended] {
        for v in 1..=8 {
            assert!(!e.defined_in(v), "version {v}");
        }
        assert!(e.defined_in(9));
    }
    let req = Envelope::request(5, Op::Stack, &());
    let reply = Envelope::reply(&req, Status::Ok, None, encode(&Frames::default()));
    assert!(!reply.defined_in(8) && reply.defined_in(9));
}

#[test]
fn bodies_round_trip() {
    same(&Start {
        target: DebugTarget::Own,
    });
    same(&Start {
        target: DebugTarget::Job { job: job() },
    });
    same(&Started { session: 7 });
    same(&AddProgram {
        program: assets(),
        r#type: "*PGM".into(),
    });
    same(&ProgramAdded {
        modules: vec![DebugModule {
            name: "ASSETS".into(),
            views: View::ALL.to_vec(),
        }],
    });
    same(&SetBreakpoints {
        program: assets(),
        module: "ASSETS".into(),
        lines: vec![
            LineBreakpoint {
                line: 12,
                condition: None,
            },
            LineBreakpoint {
                line: 30,
                condition: Some("QTY > 5".into()),
            },
        ],
    });
    same(&BreakpointsSet {
        set: vec![BreakpointSet { line: 13, stmt: 13 }],
    });
    for how in ResumeHow::ALL {
        same(&Resume { how });
    }
    same(&Frames {
        frames: vec![frame(), Frame::default()],
    });
    same(&Variables {
        frame: 1,
        parent: Some("ASSET".into()),
    });
    same(&VariableList {
        variables: vec![Variable {
            name: "QTY".into(),
            r#type: "packed(7,0)".into(),
            value: "12".into(),
            expandable: false,
        }],
    });
    same(&Eval {
        frame: 0,
        expression: "NAME:x".into(),
    });
    same(&Evaluated {
        value: "'DESK'".into(),
    });
    same(&SetServiceEntry {
        program: assets(),
        user: "*CURRENT".into(),
    });
    same(&RemoveServiceEntry { program: assets() });
    let m = ServerMessage {
        id: "MCH1211".into(),
        severity: 40,
        data: Vec::new(),
        text: "divide by zero".into(),
        help: String::new(),
    };
    for reason in [
        StopReason::Breakpoint,
        StopReason::Step,
        StopReason::ServiceEntry,
        StopReason::Exception { message: m.clone() },
        StopReason::ProgramEnd,
    ] {
        same(&Stopped {
            job: job(),
            reason,
            frame: frame(),
        });
    }
    same(&DebugEnded { message: Some(m) });
}

#[test]
fn enums_by_place() {
    assert_eq!(encode(&DebugTarget::Own), [0]);
    assert_eq!(encode(&View::Listing), [2]);
    for (i, h) in ResumeHow::ALL.iter().enumerate() {
        assert_eq!(encode(h), [u8::try_from(i).unwrap()]);
    }
    assert_eq!(encode(&StopReason::ProgramEnd), [4]);
    assert!(decode::<ResumeHow>(&[4]).is_err());
    assert!(decode::<View>(&[3]).is_err());
    assert!(decode::<StopReason>(&[5]).is_err());
    assert!(decode::<DebugTarget>(&[2]).is_err());
    // A flag other than 0 or 1 does not decode.
    let mut v = encode(&Variable::default());
    *v.last_mut().unwrap() = 2;
    assert!(decode::<Variable>(&v).is_err());
}
