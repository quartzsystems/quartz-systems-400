// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 8 (draft): `Develop`, `Jobs` and the two `Sql` lists, the
//! common source and diagnostic types, and that versions 1 to 7 are
//! what they were.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::choose_version;
use qap_proto::ops::develop::{
    Attribute, ChangeMember, CompileReply, CopyMember, CreateMember, DescribeObject, Libraries,
    Library, LibraryKind, LibraryList, ListLibraries, ListMembers, ListObjects, MemberData,
    MemberEntry, MemberRef, Members, ObjectDescription, ObjectEntry, Objects, ReadMember,
    RenameMember, SetLibraryList, WriteMember, Written,
};
use qap_proto::ops::jobs::{
    EndJob, JobEntry, JobLog, JobLogEntries, JobLogEntry, JobRef, JobState, Jobs, ListJobs,
};
use qap_proto::ops::messages::MessageType;
use qap_proto::ops::sql::{
    Index, Indexes, ListIndexes, ListRoutines, Routine, RoutineKind, Routines,
};
use qap_proto::types::{Diagnostic, EndHow, JobId, MemberName, ObjectName, SourceRecord, Where};
use qap_proto::{Envelope, Op, ServerMessage, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn assets() -> MemberName {
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
        name: "QPADEV0001".into(),
        user: "BOB".into(),
        number: "012345".into(),
    }
}

const DEVELOP: [Op; 15] = [
    Op::GetLibraryList,
    Op::SetLibraryList,
    Op::ListLibraries,
    Op::ListObjects,
    Op::DescribeObject,
    Op::ListMembers,
    Op::ReadMember,
    Op::WriteMember,
    Op::UnlockMember,
    Op::CreateMember,
    Op::CopyMember,
    Op::RenameMember,
    Op::DeleteMember,
    Op::ChangeMember,
    Op::Compile,
];

const JOBS: [Op; 5] = [
    Op::ListJobs,
    Op::JobLog,
    Op::HoldJob,
    Op::ReleaseJob,
    Op::EndJob,
];

#[test]
fn version_8_values() {
    const { assert!(VERSION >= 8) };
    assert_eq!(encode(&Service::Develop), [9]);
    assert_eq!(encode(&Service::Jobs), [10]);
    assert_eq!(Service::Develop.since(), 8);
    assert_eq!(Service::Jobs.since(), 8);
    for (code, op) in (1..=15).zip(DEVELOP) {
        assert_eq!(Op::of(Service::Develop, code), Some(op));
        assert_eq!(op.since(), 8, "{op:?}");
        assert!(!op.before_signon(), "{op:?}");
        // The library list and Compile share the job's state.
        assert_eq!(op.in_lane(), matches!(code, 1 | 2 | 15), "{op:?}");
    }
    assert_eq!(Op::of(Service::Develop, 16), None);
    for (code, op) in (1..=5).zip(JOBS) {
        assert_eq!(Op::of(Service::Jobs, code), Some(op));
        assert_eq!(op.since(), 8);
        assert!(!op.in_lane() && !op.before_signon());
    }
    assert_eq!(Op::of(Service::Jobs, 6), None);
    for (code, op) in [(12, Op::ListRoutines), (13, Op::ListIndexes)] {
        assert_eq!(Op::of(Service::Sql, code), Some(op));
        assert_eq!(op.since(), 8);
        assert!(!op.in_lane());
    }
    for v in 1..=7 {
        let s = Service::of_version(v);
        assert!(!s.contains(&Service::Develop) && !s.contains(&Service::Jobs));
        assert!(Op::ALL.iter().filter(|o| o.since() <= v).all(|o| {
            o.code().0 != Service::Develop
                && o.code().0 != Service::Jobs
                && !matches!(o, Op::ListRoutines | Op::ListIndexes)
        }));
    }
    assert_eq!(Service::of_version(8), Service::ALL[..11].to_vec());
    // Version 8 adds no status and no event.
    assert_eq!(Status::ALL.len(), 16);
    assert!(Status::ALL.iter().all(|s| s.since() <= 5));
    assert_eq!(qap_proto::ops::event::since(Service::Develop, 100), None);
    assert_eq!(qap_proto::ops::event::since(Service::Jobs, 100), None);
    let req = Envelope::request(3, Op::ListJobs, &ListJobs::default());
    let reply = Envelope::reply(&req, Status::Ok, None, Vec::new());
    assert!(!reply.defined_in(7) && reply.defined_in(8));
}

#[test]
fn versions_1_to_7_unchanged() {
    let old: Vec<(Op, (Service, u16), u16)> = Op::ALL
        .iter()
        .filter(|o| o.since() <= 7)
        .map(|o| (*o, o.code(), o.since()))
        .collect();
    assert_eq!(old.len(), 59);
    for (op, code, _) in old {
        assert_eq!(Op::of(code.0, code.1), Some(op));
    }
    // EndHow moved to the common types with version 7's values.
    assert_eq!(encode(&EndHow::Controlled), [0]);
    assert_eq!(encode(&EndHow::Immediate), [1]);
    let _: qap_proto::ops::manage::EndHow = EndHow::Immediate;
    for max in 1..=8 {
        assert_eq!(choose_version(1, max, VERSION), Some(max));
    }
    assert_eq!(choose_version(VERSION + 1, VERSION + 2, VERSION), None);
}

#[test]
fn enums_by_place() {
    assert_eq!(encode(&LibraryKind::Test), [1]);
    for (i, s) in JobState::ALL.iter().enumerate() {
        assert_eq!(encode(s), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<JobState>(&[3]).is_err());
    assert_eq!(encode(&RoutineKind::Function), [1]);
    assert_eq!(encode(&Where::None), [2]);
    assert_eq!(encode(&Where::Path { path: "/a".into() })[0], 1);
    assert!(decode::<Where>(&[3]).is_err());
}

#[test]
fn common_types_round_trip_and_convert() {
    same(&assets());
    same(&SourceRecord {
        seq: 100,
        date: 261_107,
        data: "     H DFTACTGRP(*NO)".into(),
    });
    let d = Diagnostic {
        r#where: Where::Member { member: assets() },
        line: 42,
        column: 7,
        end_line: 42,
        end_column: 12,
        severity: 30,
        id: "RNF7030".into(),
        text: "a name is not defined".into(),
        help: "define it".into(),
    };
    same(&d);
    same(&Diagnostic {
        r#where: Where::Path {
            path: "/home/BOB/x.rpgle".into(),
        },
        ..Diagnostic::default()
    });
    let c: compdiag::Diagnostic = d.clone().into();
    assert_eq!(c.line, 42);
    assert_eq!(
        c.at,
        compdiag::Where::Member(compdiag::MemberName {
            library: "HLINV".into(),
            file: "QRPGLESRC".into(),
            member: "ASSETS".into(),
        })
    );
    assert_eq!(Diagnostic::from(c), d);
    assert_eq!(Where::from(compdiag::Where::None), Where::None);
}

#[test]
#[allow(clippy::too_many_lines)] // one of each body
fn develop_bodies_round_trip() {
    let list = LibraryList {
        current: Some("HLINV".into()),
        system: vec![
            "QSYS".into(),
            "QSYS2".into(),
            "QHLPSYS".into(),
            "QUSRSYS".into(),
        ],
        user: vec!["QGPL".into(), "QTEMP".into()],
    };
    same(&list);
    same(&LibraryList::default());
    same(&SetLibraryList {
        current: None,
        user: vec!["HLINV".into()],
    });
    same(&ListLibraries {
        pattern: "HL*".into(),
        start: 1,
        max: 5,
    });
    same(&Libraries {
        total: 1,
        libraries: vec![Library {
            name: "HLINV".into(),
            kind: LibraryKind::Test,
            text: "inventory".into(),
        }],
    });
    same(&ListObjects {
        library: "*LIBL".into(),
        pattern: "*ALL".into(),
        types: vec!["*FILE".into(), "*PGM".into()],
        attribute: Some("PF".into()),
        start: 0,
        max: 0,
    });
    same(&Objects {
        total: 1,
        objects: vec![ObjectEntry {
            library: "HLINV".into(),
            name: "ASSETS".into(),
            r#type: "*PGM".into(),
            attribute: "RPGLE".into(),
            text: String::new(),
            size: 131_072,
            changed: 1_793_000_000_000,
            owner: "BOB".into(),
        }],
    });
    same(&DescribeObject {
        object: ObjectName {
            library: "*LIBL".into(),
            name: "ASSETS".into(),
        },
        r#type: "*PGM".into(),
    });
    same(&ObjectDescription {
        attributes: vec![Attribute {
            name: "Owner".into(),
            value: "BOB".into(),
        }],
    });
    same(&ListMembers {
        file: assets().file,
        pattern: "A*".into(),
        start: 0,
        max: 10,
    });
    same(&Members {
        total: 1,
        members: vec![MemberEntry {
            name: "ASSETS".into(),
            srctype: "RPGLE".into(),
            text: "assets".into(),
            records: 42,
            created: 1,
            changed: 2,
        }],
    });
    same(&ReadMember {
        member: assets(),
        lock: true,
    });
    same(&MemberData {
        srctype: "RPGLE".into(),
        text: "assets".into(),
        width: 100,
        ccsid: 37,
        records: vec![SourceRecord::default()],
    });
    same(&WriteMember {
        member: assets(),
        records: vec![SourceRecord {
            seq: 100,
            date: 0,
            data: "**FREE".into(),
        }],
        renumber: true,
        srctype: None,
        text: Some("assets".into()),
        first: true,
        last: false,
    });
    same(&Written { records: 7 });
    same(&MemberRef { member: assets() });
    same(&CreateMember {
        member: assets(),
        srctype: "RPGLE".into(),
        text: String::new(),
    });
    same(&CopyMember {
        from: assets(),
        to: assets(),
        replace: true,
    });
    same(&RenameMember {
        member: assets(),
        new_name: "ASSETS2".into(),
    });
    same(&ChangeMember {
        member: assets(),
        srctype: Some("SQLRPGLE".into()),
        text: None,
    });
    same(&CommandText {
        command: "CRTBNDRPG PGM(HLINV/ASSETS)".into(),
    });
    same(&CompileReply {
        job: job(),
        messages: vec![ServerMessage {
            id: "RNS9310".into(),
            severity: 0,
            data: vec![1, 2],
            text: "done".into(),
            help: String::new(),
        }],
        diagnostics: vec![Diagnostic::default()],
    });
    // A flag other than 0 or 1 does not decode.
    let mut b = encode(&ReadMember {
        member: assets(),
        lock: true,
    });
    *b.last_mut().unwrap() = 2;
    assert!(decode::<ReadMember>(&b).is_err());
}

#[test]
fn jobs_and_sql_bodies_round_trip() {
    same(&ListJobs {
        user: "*CURRENT".into(),
        status: Some(JobState::OnJobQueue),
        start: 0,
        max: 20,
    });
    same(&Jobs {
        total: 1,
        jobs: vec![JobEntry {
            id: job(),
            state: JobState::Active,
            status: "*ACTIVE".into(),
            r#type: "INTER".into(),
            subsystem: "QINTER".into(),
            function: "MNU-MAIN".into(),
            entered: 1_793_000_000_000,
        }],
    });
    same(&JobLog {
        job: job(),
        start: 0,
        max: 0,
    });
    same(&JobLogEntries {
        total: 1,
        entries: vec![JobLogEntry {
            message: ServerMessage::default(),
            kind: MessageType::Escape,
            sent: 5,
            from_program: "QCMD".into(),
            to_program: "QCMD".into(),
        }],
    });
    same(&JobRef { job: job() });
    same(&EndJob {
        job: job(),
        how: EndHow::Immediate,
        delay: None,
    });
    same(&ListRoutines {
        schema: "HLINV".into(),
        pattern: "%".into(),
        kinds: vec![RoutineKind::Procedure],
        start: 0,
        max: 0,
    });
    same(&Routines {
        total: 1,
        routines: vec![Routine {
            schema: "HLINV".into(),
            name: "RESTOCK".into(),
            specific: "RESTOCK".into(),
            kind: RoutineKind::Procedure,
            text: String::new(),
        }],
    });
    same(&ListIndexes {
        schema: "HLINV".into(),
        table: "ASSETS".into(),
    });
    same(&Indexes {
        indexes: vec![Index {
            schema: "HLINV".into(),
            name: "ASSETSI1".into(),
            unique: true,
            columns: vec!["ASSETID".into()],
        }],
    });
}
