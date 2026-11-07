// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 8 (draft): the `Jobs` service and `Sql` 12 and 13 against
//! the test system, one test per operation.

mod common;

use qap_proto::ops::jobs::{EndJob, JobLog, JobLogEntries, JobRef, JobState, Jobs, ListJobs};
use qap_proto::ops::messages::MessageType;
use qap_proto::ops::sql::{Indexes, ListIndexes, ListRoutines, RoutineKind, Routines};
use qap_proto::types::{EndHow, JobId};
use qap_proto::{Envelope, Op, Status};

use common::{Run, cfg, start};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    r.client.hello_range(8, 8);
    r.client.sign_on(user, password);
    r
}

fn id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

fn job(name: &str, user: &str, number: &str) -> JobId {
    JobId {
        name: name.into(),
        user: user.into(),
        number: number.into(),
    }
}

fn list(r: &mut Run, user: &str, status: Option<JobState>) -> Envelope {
    r.client.call(
        Op::ListJobs,
        &ListJobs {
            user: user.into(),
            status,
            start: 0,
            max: 0,
        },
    )
}

#[test]
fn list_jobs() {
    let mut bob = signed_on("BOB", "SECRET1");
    let j: Jobs = list(&mut bob, "*CURRENT", None).body_as().unwrap();
    let names: Vec<&str> = j.jobs.iter().map(|j| j.id.name.as_str()).collect();
    assert_eq!(names, ["QPADEV0001", "NIGHTLY", "BUILD", "OLDJOB"]);
    assert_eq!(
        (j.jobs[0].status.as_str(), j.jobs[0].subsystem.as_str()),
        ("*ACTIVE", "QINTER")
    );
    let q: Jobs = list(&mut bob, "BOB", Some(JobState::OnJobQueue))
        .body_as()
        .unwrap();
    assert_eq!((q.total, q.jobs[0].status.as_str()), (1, "*JOBQ"));
    let e = list(&mut bob, "ANN", None);
    assert_eq!(
        (e.status, id(&e).as_str()),
        (Status::NotAuthorized, "CPF1343")
    );
    let e = list(&mut bob, "*ALL", None);
    assert_eq!(e.status, Status::NotAuthorized);
    let mut oper = signed_on("OPER", "OPERPW1");
    let e = list(&mut oper, "NOBODY", None);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF2204"));
    let all: Jobs = list(&mut oper, "*ALL", None).body_as().unwrap();
    assert_eq!(all.total, 5);
    let page: Jobs = oper
        .client
        .call(
            Op::ListJobs,
            &ListJobs {
                user: "*ALL".into(),
                status: None,
                start: 1,
                max: 1,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!((page.total, page.jobs[0].id.user.as_str()), (5, "ANN"));
}

#[test]
fn job_log() {
    let mut bob = signed_on("BOB", "SECRET1");
    let log = |r: &mut Run, j: JobId| {
        r.client.call(
            Op::JobLog,
            &JobLog {
                job: j,
                start: 1,
                max: 0,
            },
        )
    };
    let l: JobLogEntries = log(&mut bob, job("BUILD", "BOB", "000150"))
        .body_as()
        .unwrap();
    assert_eq!((l.total, l.entries.len()), (3, 2));
    assert_eq!(l.entries[1].kind, MessageType::Completion);
    assert_eq!(l.entries[1].message.id, "CPF1164");
    let e = log(&mut bob, job("OLDJOB", "BOB", "000151"));
    assert_eq!(e.status, Status::NotFound);
    let e = log(&mut bob, job("PAYRUN", "ANN", "000300"));
    assert_eq!(e.status, Status::NotAuthorized);
    let e = log(&mut bob, job("NOSUCH", "BOB", "999999"));
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF1321"));
}

#[test]
fn hold_and_release_job() {
    let mut bob = signed_on("BOB", "SECRET1");
    let nightly = || JobRef {
        job: job("NIGHTLY", "BOB", "000202"),
    };
    assert_eq!(bob.client.call(Op::HoldJob, &nightly()).status, Status::Ok);
    assert!(bob.c.jobs.lock().unwrap().jobs[2].held);
    assert_eq!(
        bob.client.call(Op::ReleaseJob, &nightly()).status,
        Status::Ok
    );
    assert!(!bob.c.jobs.lock().unwrap().jobs[2].held);
    let ended = JobRef {
        job: job("BUILD", "BOB", "000150"),
    };
    let e = bob.client.call(Op::HoldJob, &ended);
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPF1362"));
    let anns = JobRef {
        job: job("PAYRUN", "ANN", "000300"),
    };
    assert_eq!(
        bob.client.call(Op::ReleaseJob, &anns).status,
        Status::NotAuthorized
    );
}

#[test]
fn end_job() {
    let mut bob = signed_on("BOB", "SECRET1");
    let end = |r: &mut Run, j: JobId| {
        r.client.call(
            Op::EndJob,
            &EndJob {
                job: j,
                how: EndHow::Controlled,
                delay: Some(30),
            },
        )
    };
    assert_eq!(
        end(&mut bob, job("NIGHTLY", "BOB", "000202")).status,
        Status::Ok
    );
    let q: Jobs = list(&mut bob, "*CURRENT", Some(JobState::OnOutputQueue))
        .body_as()
        .unwrap();
    assert_eq!(q.total, 3);
    assert_eq!(
        end(&mut bob, job("PAYRUN", "ANN", "000300")).status,
        Status::NotAuthorized
    );
    // The connection's own job: sign off instead.
    let own = job("QAPCNN", "QUSER", &format!("{:06}", bob.job));
    assert_eq!(end(&mut bob, own).status, Status::InvalidRequest);
    let mut oper = signed_on("OPER", "OPERPW1");
    let e = list(&mut oper, "NOBODY", None);
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPF2204"));
    assert_eq!(
        end(&mut oper, job("PAYRUN", "ANN", "000300")).status,
        Status::Ok
    );
}

#[test]
fn list_routines() {
    let mut bob = signed_on("BOB", "SECRET1");
    let call = |r: &mut Run, pattern: &str, kinds: Vec<RoutineKind>| {
        r.client.call(
            Op::ListRoutines,
            &ListRoutines {
                schema: "HLINV".into(),
                pattern: pattern.into(),
                kinds,
                start: 0,
                max: 0,
            },
        )
    };
    let all: Routines = call(&mut bob, "%", vec![]).body_as().unwrap();
    assert_eq!(all.total, 3);
    let p: Routines = call(&mut bob, "RE%", vec![RoutineKind::Procedure])
        .body_as()
        .unwrap();
    let specific: Vec<&str> = p.routines.iter().map(|r| r.specific.as_str()).collect();
    assert_eq!(specific, ["RESTOCK", "RESTOCK2"]);
    let f: Routines = call(&mut bob, "", vec![RoutineKind::Function])
        .body_as()
        .unwrap();
    assert_eq!(f.routines[0].name, "ASSETAGE");
    let e = bob.client.call(
        Op::ListRoutines,
        &ListRoutines {
            schema: "NOSUCH".into(),
            ..ListRoutines::default()
        },
    );
    assert_eq!(e.status, Status::NotFound);
}

#[test]
fn list_indexes() {
    let mut bob = signed_on("BOB", "SECRET1");
    let call = |r: &mut Run, schema: &str, table: &str| {
        r.client.call(
            Op::ListIndexes,
            &ListIndexes {
                schema: schema.into(),
                table: table.into(),
            },
        )
    };
    let i: Indexes = call(&mut bob, "HLINV", "ASSETS").body_as().unwrap();
    assert_eq!(i.indexes.len(), 2);
    assert!(i.indexes[0].unique);
    assert_eq!(i.indexes[1].columns, ["LOCATION", "ASSETID"]);
    let e = call(&mut bob, "HLPAY", "SALARIES");
    assert_eq!(
        (e.status, id(&e).as_str()),
        (Status::NotAuthorized, "SQL0551")
    );
    assert_eq!(call(&mut bob, "HLINV", "NOSUCH").status, Status::NotFound);
}
