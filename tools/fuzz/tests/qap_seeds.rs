// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `qap` corpus holds a framed seed for every `Debug` body (QAP
//! version 9): each request and both events. `QS400_UPDATE_SEEDS=1`
//! writes the missing ones.

use std::path::Path;

use fuzz::engine::run_one;
use fuzz::targets::TARGETS;
use ipc::Wire;
use qap_proto::ops::debug::{
    AddProgram, DebugEnded, DebugTarget, Eval, Frame, LineBreakpoint, RemoveServiceEntry, Resume,
    ResumeHow, SetBreakpoints, SetServiceEntry, Start, StopReason, Stopped, Variables,
};
use qap_proto::ops::event;
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, Service, Status};
use sha2::{Digest, Sha256};

fn assets() -> ObjectName {
    ObjectName {
        library: "HLINV".into(),
        name: "ASSETS".into(),
    }
}

fn request<B: Wire>(id: u32, op: Op, body: &B) -> Envelope {
    Envelope::request(id, op, body)
}

fn seeds() -> Vec<Envelope> {
    let job = JobId {
        name: "QPADEV0001".into(),
        user: "BOB".into(),
        number: "004711".into(),
    };
    vec![
        request(
            2,
            Op::DebugStart,
            &Start {
                target: DebugTarget::Own,
            },
        ),
        request(
            3,
            Op::DebugStart,
            &Start {
                target: DebugTarget::Job { job: job.clone() },
            },
        ),
        request(4, Op::DebugEnd, &()),
        request(
            5,
            Op::AddProgram,
            &AddProgram {
                program: assets(),
                r#type: "*PGM".into(),
            },
        ),
        request(
            6,
            Op::SetBreakpoints,
            &SetBreakpoints {
                program: assets(),
                module: "ASSETS".into(),
                lines: vec![
                    LineBreakpoint {
                        line: 11,
                        condition: None,
                    },
                    LineBreakpoint {
                        line: 13,
                        condition: Some("COUNT = 1".into()),
                    },
                ],
            },
        ),
        request(
            7,
            Op::DebugResume,
            &Resume {
                how: ResumeHow::StepOut,
            },
        ),
        request(8, Op::Stack, &()),
        request(
            9,
            Op::Variables,
            &Variables {
                frame: 0,
                parent: Some("ASSET".into()),
            },
        ),
        request(
            10,
            Op::Eval,
            &Eval {
                frame: 0,
                expression: "NAME:x".into(),
            },
        ),
        request(
            11,
            Op::SetServiceEntry,
            &SetServiceEntry {
                program: assets(),
                user: "*CURRENT".into(),
            },
        ),
        request(
            12,
            Op::RemoveServiceEntry,
            &RemoveServiceEntry { program: assets() },
        ),
        Envelope::event(Service::Debug, event::STOPPED, Status::Ok, None).with_body(&Stopped {
            job,
            reason: StopReason::ServiceEntry,
            frame: Frame {
                program: assets(),
                module: "ASSETS".into(),
                procedure: "ASSETS".into(),
                stmt: 9,
                line: 9,
                view: qap_proto::ops::debug::View::Source,
            },
        }),
        Envelope::event(Service::Debug, event::DEBUG_ENDED, Status::Ok, None)
            .with_body(&DebugEnded { message: None }),
    ]
}

#[test]
fn every_debug_body_has_a_seed() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/qap");
    let target = TARGETS
        .iter()
        .find(|t| t.name == "qap")
        .expect("the qap target");
    let update = std::env::var_os("QS400_UPDATE_SEEDS").is_some();
    let mut missing = Vec::new();
    for e in seeds() {
        let mut framed = Vec::new();
        e.write_to(&mut framed).unwrap();
        assert!(run_one(target, &framed).is_none(), "{e:?}");
        let name = hex::encode(&Sha256::digest(&framed)[..8]);
        let path = dir.join(&name);
        if path.exists() {
            continue;
        }
        if update {
            std::fs::write(&path, &framed).unwrap();
        } else {
            missing.push(format!("{:?} {} ({name})", e.service, e.op));
        }
    }
    assert!(
        missing.is_empty(),
        "seeds missing (QS400_UPDATE_SEEDS=1 writes them): {missing:?}"
    );
}
