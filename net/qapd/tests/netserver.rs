// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 10 (draft): the `NetServer` service against the test system,
//! whose NetServer is the `QZLS*` APIs over a host in memory: one test
//! per group of operations, the APIs' authority and messages, the exit
//! point, and version 9 without it.

#![allow(clippy::doc_markdown)]

mod common;

use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::netserver::{
    Attributes, ChangeAttributes, Dialect, DisabledUsers, EnableUser, FileShare, ListSessions,
    ListShares, NetSessions, PrintShare, ServerAttributes, SessionRef, Share, ShareBody,
    ShareDetail, ShareKind, ShareRef, Shares, SpoolType, StartServer,
};
use qap_proto::types::ObjectName;
use qap_proto::{Envelope, Op, Service, Status};
use qapd::manage::ExitRequest;

use common::{Run, cfg, start, start_with};

fn signed_on(user: &str, password: &str) -> Run {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(1, 10).body_as().unwrap();
    assert!(h.services.contains(&Service::NetServer));
    r.client.sign_on(user, password);
    r
}

fn id(e: &Envelope) -> String {
    e.message.as_ref().map(|m| m.id.clone()).unwrap_or_default()
}

fn list(r: &mut Run, kind: Option<ShareKind>) -> Shares {
    r.client
        .call(
            Op::ListShares,
            &ListShares {
                kind,
                start: 0,
                max: 0,
            },
        )
        .body_as()
        .unwrap()
}

fn file(name: &str, path: &str) -> Share {
    Share {
        name: name.into(),
        text: "Team files".into(),
        detail: ShareDetail::File {
            file: FileShare {
                path: path.into(),
                read_only: true,
                max_users: Some(5),
                ..FileShare::default()
            },
        },
        ..Share::default()
    }
}

#[test]
fn shares_listed_got_and_paged() {
    let mut r = signed_on("BOB", "SECRET1");
    let all = list(&mut r, None);
    let names: Vec<&str> = all.shares.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["BOBDOCS", "PRT01", "QIBM"]);
    assert_eq!(all.shares[0].current_users, 1);
    let p = list(&mut r, Some(ShareKind::Print));
    assert_eq!((p.total, p.shares[0].name.as_str()), (1, "PRT01"));
    let page: Shares = r
        .client
        .call(
            Op::ListShares,
            &ListShares {
                kind: None,
                start: 1,
                max: 1,
            },
        )
        .body_as()
        .unwrap();
    assert_eq!((page.total, page.shares[0].name.as_str()), (3, "PRT01"));
    let s: Share = r
        .client
        .call(
            Op::GetShare,
            &ShareRef {
                name: "qibm".into(),
            },
        )
        .body_as()
        .unwrap();
    assert!(matches!(&s.detail, ShareDetail::File { file } if file.path == "/QIBM"));
    let e = r.client.call(
        Op::GetShare,
        &ShareRef {
            name: "NONE".into(),
        },
    );
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPFB68A"));
}

#[test]
fn shares_added_changed_and_removed_as_the_apis_allow() {
    // BOB owns /home/BOB: he may share it, not /home.
    let mut bob = signed_on("BOB", "SECRET1");
    let ok = bob.client.call(
        Op::AddShare,
        &ShareBody {
            share: file("mine", "/home/BOB"),
        },
    );
    assert_eq!(ok.status, Status::Ok, "{:?}", ok.message);
    let no = bob.client.call(
        Op::AddShare,
        &ShareBody {
            share: file("ALL", "/home"),
        },
    );
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "CPFB684")
    );
    let dup = bob.client.call(
        Op::AddShare,
        &ShareBody {
            share: file("MINE", "/home/BOB"),
        },
    );
    assert_eq!((dup.status, id(&dup).as_str()), (Status::Exists, "CPFB68A"));
    // Change it to read/write without a maximum.
    let mut s = file("MINE", "/home/BOB");
    if let ShareDetail::File { file } = &mut s.detail {
        file.read_only = false;
        file.max_users = None;
    }
    let ok = bob
        .client
        .call(Op::ChangeShare, &ShareBody { share: s.clone() });
    assert_eq!(ok.status, Status::Ok, "{:?}", ok.message);
    let got: Share = bob
        .client
        .call(
            Op::GetShare,
            &ShareRef {
                name: "MINE".into(),
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(got.detail, s.detail);
    // A print share by SECOFR (*IOSYSCFG).
    let mut sec = start_with(cfg(), bob.c.clone());
    sec.client.hello_range(10, 10);
    sec.client.sign_on("SECOFR", "SECPW01");
    let p = Share {
        name: "LAB".into(),
        detail: ShareDetail::Print {
            print: PrintShare {
                output_queue: ObjectName {
                    library: "QUSRSYS".into(),
                    name: "PRT01".into(),
                },
                driver: "Generic PCL 6".into(),
                spool_type: SpoolType::Scs,
                printer_file: None,
            },
        },
        ..Share::default()
    };
    let ok = sec.client.call(Op::AddShare, &ShareBody { share: p });
    assert_eq!(ok.status, Status::Ok, "{:?}", ok.message);
    // Remove.
    let ok = bob.client.call(
        Op::RemoveShare,
        &ShareRef {
            name: "MINE".into(),
        },
    );
    assert_eq!(ok.status, Status::Ok);
    let e = bob.client.call(
        Op::RemoveShare,
        &ShareRef {
            name: "MINE".into(),
        },
    );
    assert_eq!(e.status, Status::NotFound);
}

#[test]
fn sessions_listed_and_ended() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let s: NetSessions = r
        .client
        .call(Op::ListSessions, &ListSessions::default())
        .body_as()
        .unwrap();
    assert_eq!(s.total, 2);
    assert_eq!(s.sessions[0].user, "BOB");
    assert_eq!(s.sessions[0].connections[0].open_files, 2);
    assert!(s.sessions[1].guest);
    assert_eq!(
        s.sessions[0].job.as_ref().map(|j| j.name.as_str()),
        Some("QZLSFILE")
    );
    // BOB has no *IOSYSCFG.
    let mut bob = start_with(cfg(), r.c.clone());
    bob.client.hello_range(10, 10);
    bob.client.sign_on("BOB", "SECRET1");
    let id1 = s.sessions[0].id;
    let no = bob.client.call(Op::EndSession, &SessionRef { id: id1 });
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "CPFB684")
    );
    let ok = r.client.call(Op::EndSession, &SessionRef { id: id1 });
    assert_eq!(ok.status, Status::Ok);
    let e = r.client.call(Op::EndSession, &SessionRef { id: 999 });
    assert_eq!((e.status, id(&e).as_str()), (Status::NotFound, "CPFB68E"));
}

#[test]
fn attributes_current_and_pending() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let a: Attributes = r
        .client
        .call(Op::GetServerAttributes, &())
        .body_as()
        .unwrap();
    assert!(a.active);
    assert_eq!(a.job.as_ref().map(|j| j.name.as_str()), Some("QZLSSERVER"));
    assert_eq!(a.current, a.pending);
    let want = ServerAttributes {
        domain: "LAB".into(),
        idle_timeout: 900,
        min_dialect: Dialect::Smb311,
        allow_netbios: true,
        ..a.pending.clone()
    };
    let now: ServerAttributes = r
        .client
        .call(
            Op::ChangeServerAttributes,
            &ChangeAttributes {
                pending: want.clone(),
            },
        )
        .body_as()
        .unwrap();
    assert_eq!(now, want);
    let a: Attributes = r
        .client
        .call(Op::GetServerAttributes, &())
        .body_as()
        .unwrap();
    assert_eq!(
        (a.current.idle_timeout, a.pending.idle_timeout),
        (6000, 900)
    );
    // A value the API refuses, and a user without *IOSYSCFG.
    let bad = ServerAttributes {
        oplock_timeout: 9999,
        ..want
    };
    let e = r.client.call(
        Op::ChangeServerAttributes,
        &ChangeAttributes {
            pending: bad.clone(),
        },
    );
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPFB686"));
    let mut bob = start_with(cfg(), r.c.clone());
    bob.client.hello_range(10, 10);
    bob.client.sign_on("BOB", "SECRET1");
    let e = bob.client.call(
        Op::ChangeServerAttributes,
        &ChangeAttributes { pending: bad },
    );
    assert_eq!(e.status, Status::NotAuthorized);
}

#[test]
fn disabled_users_start_and_end() {
    let mut r = signed_on("SECOFR", "SECPW01");
    let d: DisabledUsers = r.client.call(Op::ListDisabledUsers, &()).body_as().unwrap();
    assert_eq!(d.users[0].user, "ANN");
    let mut oper = start_with(cfg(), r.c.clone());
    oper.client.hello_range(10, 10);
    oper.client.sign_on("OPER", "OPERPW1");
    let e = oper
        .client
        .call(Op::EnableUser, &EnableUser { user: "ANN".into() });
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPFB694"));
    let ok = r
        .client
        .call(Op::EnableUser, &EnableUser { user: "ann".into() });
    assert_eq!(ok.status, Status::Ok);
    let d: DisabledUsers = r.client.call(Op::ListDisabledUsers, &()).body_as().unwrap();
    assert!(d.users.is_empty());
    // Active: a start is refused; end, then start with reset.
    let e = r
        .client
        .call(Op::StartServer, &StartServer { reset: false });
    assert_eq!((e.status, id(&e).as_str()), (Status::Error, "CPFB685"));
    assert_eq!(r.client.call(Op::EndServer, &()).status, Status::Ok);
    let ok = r.client.call(Op::StartServer, &StartServer { reset: true });
    assert_eq!(ok.status, Status::Ok);
}

#[test]
fn the_exit_point_sees_netserver_requests() {
    let mut r = signed_on("SECOFR", "SECPW01");
    r.c.manage.lock().unwrap().exit_refuses =
        Some(Box::new(|e: &ExitRequest| e.name == "RemoveShare"));
    let no = r.client.call(
        Op::RemoveShare,
        &ShareRef {
            name: "QIBM".into(),
        },
    );
    assert_eq!(
        (no.status, id(&no).as_str()),
        (Status::NotAuthorized, "TCP1A5E")
    );
    assert_eq!(list(&mut r, None).total, 3);
    let calls = r.c.manage.lock().unwrap().exit_calls.clone();
    let last = calls.last().unwrap();
    assert_eq!(ccsid::from_ebcdic(&last[10..20]), "*QAPNETSVR");
    assert_eq!(last[28..32], [0, 0, 0, 1]);
}

#[test]
fn version_9_has_no_netserver() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello_range(9, 9).body_as().unwrap();
    assert!(!h.services.contains(&Service::NetServer));
    r.client.sign_on("SECOFR", "SECPW01");
    let e = r.client.call(Op::ListShares, &ListShares::default());
    assert_eq!(e.status, Status::InvalidRequest);
}
