// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection rules of docs/QAP.md: Hello first and once, versions,
//! Ping, the operations refused before sign-on, 64 outstanding requests,
//! Cancel, frames over 1 MiB, the idle close and `ServerEnding`.

mod common;

use std::time::Duration;

use qap_proto::ops::connection::{Hello, HelloReply, PingReply, SignOnMethod};
use qap_proto::ops::event;
use qap_proto::{Envelope, Kind, MAX_FRAME, Op, Service, Status};
use qapd::{Closed, Config, Ending};

use common::{cfg, start};

#[test]
fn hello_then_ping() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello().body_as().unwrap();
    assert_eq!(h.version, 1);
    assert_eq!(h.system_name, "QSHQ400");
    assert_eq!(h.services, Service::of_version(1));
    assert_eq!(h.signon_methods, vec![SignOnMethod::Password]);
    assert_eq!(h.system_ccsid, 37);
    let p = r.client.call(Op::Ping, &());
    assert_eq!(p.status, Status::Ok);
    assert_eq!(
        p.body_as::<PingReply>().unwrap().server_time,
        1_793_000_000_000
    );
    drop(r.client);
    assert_eq!(r.server.join().unwrap(), Closed::Peer);
}

#[test]
fn version_mismatch_closes() {
    let mut r = start(cfg());
    let rep = r.client.call(
        Op::Hello,
        &Hello {
            min_version: qap_proto::VERSION + 1,
            max_version: 9,
            ..Hello::default()
        },
    );
    assert_eq!(rep.status, Status::VersionUnsupported);
    assert!(r.client.closed());
    assert_eq!(r.server.join().unwrap(), Closed::Refused);
}

#[test]
fn hello_must_be_first_with_id_1() {
    let mut r = start(cfg());
    assert_eq!(r.client.call(Op::Ping, &()).status, Status::InvalidRequest);
    assert!(r.client.closed());
    let mut r = start(cfg());
    r.client.set_next_id(6);
    assert_eq!(
        r.client.call(Op::Hello, &Hello::default()).status,
        Status::InvalidRequest
    );
    assert!(r.client.closed());
}

#[test]
fn a_second_hello_closes() {
    let mut r = start(cfg());
    r.client.hello();
    r.client.set_next_id(1);
    assert_eq!(
        r.client.call(Op::Hello, &Hello::default()).status,
        Status::InvalidRequest
    );
    assert!(r.client.closed());
}

#[test]
fn signed_on_operations_are_refused() {
    let mut r = start(cfg());
    r.client.hello();
    assert_eq!(
        r.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
    assert_eq!(r.client.call(Op::SignOff, &()).status, Status::NotSignedOn);
    let unknown = Envelope {
        op: 77,
        ..Envelope::request(9, Op::Ping, &())
    };
    r.client.send(&unknown);
    assert_eq!(r.client.reply(9).status, Status::InvalidRequest);
}

#[test]
fn more_than_64_outstanding_is_busy() {
    let mut r = start(cfg());
    r.client.hello();
    r.client.sign_on("BOB", "SECRET1");
    r.c.gate.set(false);
    let ids: Vec<u32> = (0..65)
        .map(|_| r.client.request(Op::SystemInfo, &()))
        .collect();
    // The 65th is refused at once; the first 64 wait.
    let busy = r.client.recv().unwrap();
    assert_eq!((busy.request_id, busy.status), (ids[64], Status::Busy));
    r.c.gate.set(true);
    assert!(
        r.client
            .replies(&ids[..64])
            .values()
            .all(|e| e.status == Status::Ok)
    );
    // A request ID still outstanding cannot be used again.
    r.c.gate.set(false);
    let a = r.client.request(Op::SystemInfo, &());
    r.client.send(&Envelope::request(a, Op::SystemInfo, &()));
    let dup = r.client.recv().unwrap();
    assert_eq!((dup.request_id, dup.status), (a, Status::InvalidRequest));
    r.c.gate.set(true);
    assert_eq!(r.client.reply(a).status, Status::Ok);
}

#[test]
fn cancel_running_and_waiting() {
    let mut r = start(cfg());
    r.client.hello_range(1, 2);
    r.client.sign_on("BOB", "SECRET1");
    let text = |c: &str| qap_proto::ops::command::CommandText { command: c.into() };
    // Running: held at the gate. Waiting: a command behind a command.
    r.c.gate.set(false);
    let running = r.client.request(Op::SystemInfo, &());
    let first = r.client.request(Op::RunCommand, &text("DLYJOB DLY(1)"));
    let waiting = r.client.request(Op::RunCommand, &text("CRTLIB LIB(NEVER)"));
    std::thread::sleep(Duration::from_millis(100));
    r.client.send(&Envelope::cancel(waiting));
    r.client.send(&Envelope::cancel(running));
    // A cancel naming nothing outstanding is ignored.
    r.client.send(&Envelope::cancel(999));
    std::thread::sleep(Duration::from_millis(100));
    r.c.gate.set(true);
    let got = r.client.replies(&[running, first, waiting]);
    let (a, b) = (&got[&running], &got[&waiting]);
    assert_eq!((a.status, b.status), (Status::Cancelled, Status::Cancelled));
    assert_eq!(got[&first].status, Status::Ok);
    // Started: its own body. Not started: none, and it never ran.
    assert!(!a.body.is_empty());
    assert!(b.body.is_empty());
    assert!(r.c.libraries.lock().unwrap().is_empty());
    assert_eq!(r.client.call(Op::Ping, &()).status, Status::Ok);
}

#[test]
fn frames_over_1_mib_close() {
    let mut r = start(cfg());
    r.client.hello();
    r.client
        .raw(&u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes());
    assert!(r.client.closed());
    assert_eq!(r.server.join().unwrap(), Closed::BadFrame);
}

#[test]
fn idle_connections_close() {
    let mut r = start(Config {
        idle_close: Duration::from_millis(300),
        ..cfg()
    });
    r.client.hello();
    assert!(r.client.closed());
    assert_eq!(r.server.join().unwrap(), Closed::Idle);
}

#[test]
fn server_ending_is_announced() {
    let mut r = start(cfg());
    r.client.hello();
    *r.c.ending.lock().unwrap() = Some(Ending::Server);
    let e = r.client.recv().unwrap();
    assert_eq!(
        (e.kind, e.request_id, e.service, e.op),
        (Kind::Event, 0, Service::Connection, event::SERVER_ENDING)
    );
    assert_eq!(e.status, Status::ServerEnding);
    assert_eq!(e.message.unwrap().id, "TCP1A0A");
    assert!(r.client.closed());
    assert_eq!(r.server.join().unwrap(), Closed::ServerEnding);
}
