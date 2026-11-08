// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `qap` target's corpus holds a seed for every request body QAP
//! version 8 adds: one framed request each, named as the fuzzer names
//! corpus entries. `QS400_UPDATE_SEEDS=1` writes the missing ones.

use std::path::Path;

use ipc::Wire;
use qap_proto::ops::command::CommandText;
use qap_proto::ops::develop::{
    ChangeMember, CopyMember, CreateMember, DescribeObject, ListLibraries, ListMembers,
    ListObjects, MemberRef, ReadMember, RenameMember, SetLibraryList, WriteMember,
};
use qap_proto::ops::jobs::{EndJob, JobLog, JobRef, JobState, ListJobs};
use qap_proto::ops::sql::{ListIndexes, ListRoutines, RoutineKind};
use qap_proto::types::{EndHow, JobId, MemberName, ObjectName, SourceRecord};
use qap_proto::{Envelope, Op};
use sha2::{Digest, Sha256};

fn frame<B: Wire>(op: Op, body: &B) -> Vec<u8> {
    let mut b = Vec::new();
    Envelope::request(7, op, body).write_to(&mut b).unwrap();
    b
}

fn member() -> MemberName {
    MemberName {
        file: ObjectName {
            library: "HLINV".into(),
            name: "QRPGLESRC".into(),
        },
        member: "ASSETS".into(),
    }
}

fn job() -> JobId {
    JobId {
        name: "BUILD".into(),
        user: "BOB".into(),
        number: "000150".into(),
    }
}

/// One framed request per version 8 body.
#[allow(clippy::too_many_lines)] // one of each
fn seeds() -> Vec<Vec<u8>> {
    let rec = SourceRecord {
        seq: 100,
        date: 261_107,
        data: "**FREE".into(),
    };
    vec![
        frame(Op::GetLibraryList, &()),
        frame(
            Op::SetLibraryList,
            &SetLibraryList {
                current: Some("HLINV".into()),
                user: vec!["QGPL".into(), "QTEMP".into()],
            },
        ),
        frame(
            Op::ListLibraries,
            &ListLibraries {
                pattern: "HL*".into(),
                start: 0,
                max: 50,
            },
        ),
        frame(
            Op::ListObjects,
            &ListObjects {
                library: "*LIBL".into(),
                pattern: "ASSET*".into(),
                types: vec!["*FILE".into(), "*PGM".into()],
                attribute: Some("RPGLE".into()),
                start: 0,
                max: 0,
            },
        ),
        frame(
            Op::DescribeObject,
            &DescribeObject {
                object: member().file,
                r#type: "*FILE".into(),
            },
        ),
        frame(
            Op::ListMembers,
            &ListMembers {
                file: member().file,
                pattern: String::new(),
                start: 0,
                max: 0,
            },
        ),
        frame(
            Op::ReadMember,
            &ReadMember {
                member: member(),
                lock: true,
            },
        ),
        frame(
            Op::WriteMember,
            &WriteMember {
                member: member(),
                records: vec![rec.clone(), rec],
                renumber: true,
                srctype: Some("RPGLE".into()),
                text: None,
                first: true,
                last: true,
            },
        ),
        frame(Op::UnlockMember, &MemberRef { member: member() }),
        frame(
            Op::CreateMember,
            &CreateMember {
                member: member(),
                srctype: "RPGLE".into(),
                text: "new".into(),
            },
        ),
        frame(
            Op::CopyMember,
            &CopyMember {
                from: member(),
                to: member(),
                replace: false,
            },
        ),
        frame(
            Op::RenameMember,
            &RenameMember {
                member: member(),
                new_name: "ASSETS2".into(),
            },
        ),
        frame(Op::DeleteMember, &MemberRef { member: member() }),
        frame(
            Op::ChangeMember,
            &ChangeMember {
                member: member(),
                srctype: None,
                text: Some("text".into()),
            },
        ),
        frame(
            Op::Compile,
            &CommandText {
                command: "CRTBNDRPG PGM(HLINV/ASSETS)".into(),
            },
        ),
        frame(
            Op::ListJobs,
            &ListJobs {
                user: "*CURRENT".into(),
                status: Some(JobState::OnOutputQueue),
                start: 0,
                max: 0,
            },
        ),
        frame(
            Op::JobLog,
            &JobLog {
                job: job(),
                start: 0,
                max: 100,
            },
        ),
        frame(Op::HoldJob, &JobRef { job: job() }),
        frame(Op::ReleaseJob, &JobRef { job: job() }),
        frame(
            Op::EndJob,
            &EndJob {
                job: job(),
                how: EndHow::Immediate,
                delay: Some(30),
            },
        ),
        frame(
            Op::ListRoutines,
            &ListRoutines {
                schema: "HLINV".into(),
                pattern: "%".into(),
                kinds: vec![RoutineKind::Procedure, RoutineKind::Function],
                start: 0,
                max: 0,
            },
        ),
        frame(
            Op::ListIndexes,
            &ListIndexes {
                schema: "HLINV".into(),
                table: "ASSETS".into(),
            },
        ),
    ]
}

#[test]
fn version_8_bodies_are_seeded() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/qap");
    let update = std::env::var_os("QS400_UPDATE_SEEDS").is_some();
    let mut missing = Vec::new();
    for s in seeds() {
        let name = hex::encode(&Sha256::digest(&s)[..8]);
        let path = dir.join(&name);
        if path.is_file() {
            continue;
        }
        if update {
            std::fs::write(&path, &s).unwrap();
        } else {
            missing.push(name);
        }
        // The target takes every seed without a panic.
        assert!(
            fuzz::engine::run_one(
                fuzz::targets::TARGETS
                    .iter()
                    .find(|t| t.name == "qap")
                    .unwrap(),
                &s
            )
            .is_none()
        );
    }
    assert!(
        missing.is_empty(),
        "seeds missing (QS400_UPDATE_SEEDS=1 writes them): {missing:?}"
    );
}
