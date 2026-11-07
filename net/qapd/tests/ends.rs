// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's ends from outside: the server ending reaches every
//! connection, and a connection dropped mid-request gets no reply.

mod common;

use std::sync::Arc;
use std::time::Duration;

use qap_proto::ops::command::CommandText;
use qap_proto::ops::event;
use qap_proto::{Kind, Op, Service, Status};
use qapd::{Closed, Ending};

use common::{Client, Controls, Run, cfg, start_with};

/// A version 4 connection of `sys`, signed on as `BOB`.
fn bob(sys: &Arc<Controls>) -> Run {
    let mut r = start_with(cfg(), Arc::clone(sys));
    r.client.hello_range(1, 4);
    r.client.sign_on("BOB", "SECRET1");
    r
}

#[test]
fn server_ending_reaches_every_connection() {
    let sys = Controls::new();
    let mut runs: Vec<Run> = (0..4).map(|_| bob(&sys)).collect();
    // One not signed on yet ends too.
    let mut fresh = start_with(cfg(), Arc::clone(&sys));
    fresh.client.hello_range(1, 4);
    runs.push(fresh);
    sys.end(Ending::Server);
    for r in runs {
        let mut r = r;
        let e = r.client.recv().expect("ServerEnding");
        assert_eq!(
            (e.kind, e.request_id, e.service, e.op),
            (Kind::Event, 0, Service::Connection, event::SERVER_ENDING)
        );
        assert_eq!(e.status, Status::ServerEnding);
        assert!(r.client.closed());
        assert_eq!(r.server.join().unwrap(), Closed::ServerEnding);
    }
    // A connection made after the end is refused.
    let late = qapd::testing::try_serve_loopback_as(cfg(), &sys, qapd::testing::next_job());
    assert_eq!(
        late.err().map(|e| e.kind()),
        Some(std::io::ErrorKind::ConnectionRefused)
    );
}

#[test]
fn an_ended_listener_refuses_until_restarted() {
    let sys = Controls::new();
    let l = sys.listen(cfg()).expect("listen");
    let addr = l.addr();
    let mut open = Client::over(l.connect().expect("connect"));
    open.hello_range(1, 4);
    sys.end(Ending::Server);
    // Refused at connect, as the real port is once QAPD is gone.
    assert!(!l.accepting());
    let refused = l.connect();
    assert!(refused.is_err(), "connected while ended: {refused:?}");
    // The open connection gets ServerEnding, as before.
    let e = open.recv().expect("ServerEnding");
    assert_eq!(
        (e.kind, e.service, e.status),
        (Kind::Event, Service::Connection, Status::ServerEnding)
    );
    assert!(open.closed());
    assert_eq!(
        l.next_close(Duration::from_secs(10)).map(|(_, why)| why),
        Some(Closed::ServerEnding)
    );
    // A restart opens the same port again; new connections work.
    sys.restart().expect("restart");
    assert!(l.accepting());
    assert_eq!(l.addr(), addr);
    let mut again = Client::over(l.connect().expect("connect after restart"));
    assert_eq!(again.hello_range(1, 4).status, Status::Ok);
    again.sign_on("BOB", "SECRET1");
    assert_eq!(again.call(Op::Ping, &()).status, Status::Ok);
    // And a second end reaches it.
    sys.end(Ending::Server);
    assert_eq!(again.recv().map(|e| e.status), Some(Status::ServerEnding));
}

#[test]
fn a_restart_does_not_save_a_connection_from_the_end() {
    let sys = Controls::new();
    let l = sys.listen(cfg()).expect("listen");
    let mut open = Client::over(l.connect().expect("connect"));
    open.hello_range(1, 4);
    // Ended and restarted before the connection polls: it still ends.
    sys.end(Ending::Server);
    sys.restart().expect("restart");
    assert_eq!(open.recv().map(|e| e.status), Some(Status::ServerEnding));
    assert!(open.closed());
}

#[test]
fn an_operator_end_is_one_jobs() {
    let sys = Controls::new();
    let one = bob(&sys);
    let mut two = start_with(cfg(), Arc::clone(&sys));
    two.client.hello_range(1, 4);
    sys.end(Ending::Operator);
    // Whichever polls first takes it; the other stays.
    let (ended, mut other) = loop {
        if one.server.is_finished() {
            break (one, two);
        }
        if two.server.is_finished() {
            break (two, one);
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(ended.server.join().unwrap(), Closed::Operator);
    assert_eq!(other.client.call(Op::Ping, &()).status, Status::Ok);
}

#[test]
fn a_dropped_connection_gets_no_reply() {
    let sys = Controls::new();
    let mut r = bob(&sys);
    let mut other = bob(&sys);
    // The job finishes the command before its connection ends.
    let id = r.client.request(
        Op::RunCommand,
        &CommandText {
            command: "DLYJOB DLY(2)".into(),
        },
    );
    assert_eq!(
        sys.wait_running(r.job, Duration::from_secs(10)).as_deref(),
        Some("DLYJOB")
    );
    assert!(sys.drop_connection(r.job));
    // Closed under the request: no reply, no event.
    assert!(r.client.closed(), "request {id} was answered");
    let why = r.server.join().unwrap();
    assert!(matches!(why, Closed::Peer | Closed::Broken(_)), "{why:?}");
    assert!(!sys.drop_connection(r.job), "dropped once");
    // The others go on.
    assert_eq!(other.client.call(Op::Ping, &()).status, Status::Ok);
    assert_eq!(sys.drop_all(), 1);
    assert!(other.client.closed());
}
