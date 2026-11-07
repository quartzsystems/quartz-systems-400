// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 6's spool watches against the test system's printer output:
//! both target kinds, files added, changed and removed from outside and
//! through QAP, a move between two watched queues, the statuses, and
//! the end with the connection.

#![allow(clippy::many_single_char_names)] // the system, runs and events

mod common;

use std::sync::Arc;
use std::time::Duration;

use qap_proto::ops::event;
use qap_proto::ops::spool::{
    DeviceType, MoveSpooledFile, SpoolChange, SpoolStatus, SpoolTarget, SpooledFileChanged,
    SpooledFileId, SpooledFileRef, WatchSpool,
};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Kind, Op, Service, Status};
use qapd::Config;
use qapd::testing::spool::File;

use common::{Controls, Run, cfg, start_with};

fn quick() -> Config {
    Config {
        watch_poll: Duration::from_millis(20),
        ..cfg()
    }
}

fn signed_on(c: &Arc<Controls>, user: &str, password: &str, max: u16) -> Run {
    let mut r = start_with(quick(), Arc::clone(c));
    r.client.hello_range(1, max);
    r.client.sign_on(user, password);
    r
}

fn q(library: &str, name: &str) -> ObjectName {
    ObjectName {
        library: library.into(),
        name: name.into(),
    }
}

fn queue(library: &str, name: &str) -> SpoolTarget {
    SpoolTarget::Queue {
        outq: q(library, name),
    }
}

fn user(profile: &str) -> SpoolTarget {
    SpoolTarget::User {
        profile: profile.into(),
    }
}

fn watch(r: &mut Run, target: SpoolTarget) -> Envelope {
    r.client.call(Op::WatchSpool, &WatchSpool { target })
}

/// A new SCS file of `owner` named `name` on `outq`.
fn new_file(owner: &str, name: &str, number: u32, outq: ObjectName) -> File {
    File {
        id: SpooledFileId {
            job: JobId {
                name: "QPADEV0005".into(),
                user: owner.into(),
                number: "004321".into(),
            },
            file: name.into(),
            number,
        },
        status: SpoolStatus::Ready,
        outq,
        form_type: "*STD".into(),
        device: DeviceType::Scs,
        copies: 1,
        created: 1_793_500_000_000 + i64::from(number),
        data: vec![0x40; 20],
        ..File::default()
    }
}

/// The next frame, which must be a `SpooledFileChanged` event.
fn changed(r: &mut Run) -> SpooledFileChanged {
    let e = r.client.recv().expect("an event");
    assert_eq!(
        (e.kind, e.service, e.op, e.status, e.request_id),
        (
            Kind::Event,
            Service::Spool,
            event::SPOOLED_FILE_CHANGED,
            Status::Ok,
            0
        ),
        "{e:?}"
    );
    e.body_as().unwrap()
}

fn quiet(r: &mut Run) {
    if let Some(e) = r.client.try_recv(Duration::from_millis(300)) {
        panic!("nothing should come: {e:?}");
    }
}

#[test]
fn a_queue_target_sees_every_file_on_it() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1", 6);
    assert_eq!(watch(&mut r, queue("QGPL", "QPRINT")).status, Status::Ok);
    quiet(&mut r);
    // Anyone's file on the queue.
    let id = c.spool_add(new_file("ANN", "REPORT", 1, q("QGPL", "QPRINT")));
    let e = changed(&mut r);
    assert_eq!(
        (e.target.clone(), e.change, e.id.clone()),
        (queue("QGPL", "QPRINT"), SpoolChange::Added, id.clone())
    );
    let f = e.file.expect("the file");
    assert_eq!(
        (f.status, f.user.as_str(), f.outq.name.as_str()),
        (SpoolStatus::Ready, "ANN", "QPRINT")
    );
    // Status and attributes.
    assert!(c.spool_change(&id, |f| f.status = SpoolStatus::Held));
    let e = changed(&mut r);
    assert_eq!(e.change, SpoolChange::Changed);
    assert_eq!(e.file.unwrap().status, SpoolStatus::Held);
    assert!(c.spool_change(&id, |f| f.copies = 3));
    assert_eq!(changed(&mut r).file.unwrap().copies, 3);
    // Not on the queue: nothing.
    c.spool_add(new_file("ANN", "ELSEWHERE", 2, q("QGPL", "PRT01")));
    quiet(&mut r);
    assert!(c.spool_remove(&id));
    let e = changed(&mut r);
    assert_eq!((e.change, e.file, e.id), (SpoolChange::Removed, None, id));
    quiet(&mut r);
}

#[test]
fn a_user_target_sees_the_users_files_anywhere() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1", 6);
    assert_eq!(watch(&mut r, user("*CURRENT")).status, Status::Ok);
    assert_eq!(watch(&mut r, user("ANN")).status, Status::Ok);
    let mine = c.spool_add(new_file("BOB", "MINE", 1, q("QGPL", "PRT01")));
    let e = changed(&mut r);
    assert_eq!(
        (e.target, e.change, e.id.clone()),
        (user("*CURRENT"), SpoolChange::Added, mine.clone())
    );
    let hers = c.spool_add(new_file("ANN", "HERS", 2, q("QGPL", "QPRINT")));
    let e = changed(&mut r);
    assert_eq!(
        (e.target, e.change, e.id),
        (user("ANN"), SpoolChange::Added, hers)
    );
    // Through QAP too: a hold of BOB's file.
    let h = r.client.call(
        Op::HoldSpooledFile,
        &qap_proto::ops::spool::HoldSpooledFile {
            id: mine.clone(),
            immediately: true,
        },
    );
    assert_eq!(h.status, Status::Ok);
    let e = changed(&mut r);
    assert_eq!(
        (e.change, e.file.unwrap().status),
        (SpoolChange::Changed, SpoolStatus::Held)
    );
    let d = r
        .client
        .call(Op::DeleteSpooledFile, &SpooledFileRef { id: mine.clone() });
    assert_eq!(d.status, Status::Ok);
    let e = changed(&mut r);
    assert_eq!((e.change, e.id), (SpoolChange::Removed, mine));
    quiet(&mut r);
}

#[test]
fn a_move_between_watched_queues() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1", 6);
    let id = c.spool_add(new_file("BOB", "MOVEME", 1, q("QGPL", "QPRINT")));
    for t in [
        queue("QGPL", "QPRINT"),
        queue("QGPL", "PRT01"),
        user("*CURRENT"),
    ] {
        assert_eq!(watch(&mut r, t).status, Status::Ok);
    }
    quiet(&mut r);
    let m = r.client.call(
        Op::MoveSpooledFile,
        &MoveSpooledFile {
            id: id.clone(),
            outq: q("QGPL", "PRT01"),
            first: false,
        },
    );
    assert_eq!(m.status, Status::Ok);
    let mut got: Vec<(SpoolTarget, SpoolChange)> = (0..3)
        .map(|_| {
            let e = changed(&mut r);
            assert_eq!(e.id, id);
            match e.change {
                SpoolChange::Removed => assert!(e.file.is_none()),
                _ => assert_eq!(e.file.as_ref().unwrap().outq.name, "PRT01"),
            }
            (e.target, e.change)
        })
        .collect();
    got.sort_by_key(|(_, ch)| *ch as u8);
    assert_eq!(
        got,
        vec![
            (queue("QGPL", "PRT01"), SpoolChange::Added),
            (user("*CURRENT"), SpoolChange::Changed),
            (queue("QGPL", "QPRINT"), SpoolChange::Removed),
        ]
    );
    quiet(&mut r);
}

#[test]
fn statuses() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1", 6);
    let e = watch(&mut r, queue("QGPL", "NOSUCHQ"));
    assert_eq!(
        (e.status, e.message.map(|m| m.id)),
        (Status::NotFound, Some("CPF3357".into()))
    );
    let e = watch(&mut r, queue("QUSRSYS", "PRIVATE"));
    assert_eq!(e.status, Status::NotAuthorized);
    let e = watch(&mut r, user("NOBODY"));
    assert_eq!(
        (e.status, e.message.map(|m| m.id)),
        (Status::NotFound, Some("CPF2204".into()))
    );
    assert_eq!(watch(&mut r, user("*ALL")).status, Status::InvalidRequest);
    let e = r.client.call(
        Op::UnwatchSpool,
        &WatchSpool {
            target: user("*CURRENT"),
        },
    );
    assert_eq!(
        (e.status, e.message.map(|m| m.id)),
        (Status::NotFound, Some("TCP1A5B".into()))
    );
    assert_eq!(watch(&mut r, user("*CURRENT")).status, Status::Ok);
    assert_eq!(
        r.client
            .call(
                Op::UnwatchSpool,
                &WatchSpool {
                    target: user("*CURRENT")
                }
            )
            .status,
        Status::Ok
    );
    c.spool_add(new_file("BOB", "AFTER", 1, q("QGPL", "QPRINT")));
    quiet(&mut r);
    // Version 5: no watches.
    let mut old = signed_on(&c, "BOB", "SECRET1", 5);
    assert_eq!(
        watch(&mut old, user("*CURRENT")).status,
        Status::InvalidRequest
    );
}

#[test]
fn a_watch_ends_with_its_connection() {
    let c = Controls::new();
    let mut r = signed_on(&c, "BOB", "SECRET1", 6);
    assert_eq!(watch(&mut r, queue("QGPL", "QPRINT")).status, Status::Ok);
    drop(r.client);
    r.server.join().unwrap();
    // The system goes on; nothing is left watching.
    let mut n = signed_on(&c, "BOB", "SECRET1", 6);
    c.spool_add(new_file("BOB", "NOBODY", 1, q("QGPL", "QPRINT")));
    quiet(&mut n);
    // A sign-off ends a watch too.
    assert_eq!(watch(&mut n, queue("QGPL", "QPRINT")).status, Status::Ok);
    assert_eq!(n.client.call(Op::SignOff, &()).status, Status::Ok);
    n.client.sign_on("BOB", "SECRET1");
    c.spool_add(new_file("BOB", "STILL", 2, q("QGPL", "QPRINT")));
    quiet(&mut n);
}
