// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Attach` (version 4): its statuses, a job and job state of its own
//! per connection, at most 16 connections a session, `SignOff` on one
//! ending the others, the idle timer only without connections, `Resume`
//! beside attached connections, and an operator ending one of them.

mod common;

use std::sync::Arc;

use qap_proto::ops::command::{CommandReply, CommandText};
use qap_proto::ops::event;
use qap_proto::ops::signon::{Attach, Resume, SignOn, SignOnReply};
use qap_proto::{Envelope, Kind, Op, Secret, Service, Status, Token};
use qapd::{Closed, Ending};

use common::{Controls, Run, cfg, start_with};

/// A connection that said `Hello` for versions 1 to `max`.
fn hello(sys: &Arc<Controls>, max: u16) -> Run {
    let mut r = start_with(cfg(), Arc::clone(sys));
    r.client.hello_range(1, max);
    r
}

/// A version 4 connection signed on as `BOB`, and its token.
fn bob(sys: &Arc<Controls>) -> (Run, Token) {
    let mut r = hello(sys, 4);
    let s = r.client.call(
        Op::SignOn,
        &SignOn {
            user: "BOB".into(),
            password: Secret::from("SECRET1"),
        },
    );
    assert_eq!(s.status, Status::Ok);
    let token = s.body_as::<SignOnReply>().unwrap().token;
    (r, token)
}

fn attach(r: &mut Run, token: &Token) -> Envelope {
    r.client.call(Op::Attach, &Attach { token: token.clone() })
}

/// A new version 4 connection attached to `token`'s session.
fn attached(sys: &Arc<Controls>, token: &Token) -> Run {
    let mut r = hello(sys, 4);
    let got = attach(&mut r, token);
    assert_eq!(got.status, Status::Ok, "{got:?}");
    r
}

/// The status and the message's ID.
fn status_msg(got: &Envelope) -> (Status, String) {
    (got.status, got.message.as_ref().map(|m| m.id.clone()).unwrap_or_default())
}

/// Close the client and wait for its job to detach.
fn close(r: Run) {
    drop(r.client);
    assert_eq!(r.server.join().unwrap(), Closed::Peer);
}

/// Run `command`; its reply.
fn run(r: &mut Run, command: &str) -> (Status, CommandReply) {
    let got = r.client.call(
        Op::RunCommand,
        &CommandText {
            command: command.into(),
        },
    );
    (got.status, got.body_as().unwrap())
}

fn job_number(reply: &CommandReply) -> u32 {
    reply.job.number.parse().unwrap()
}

/// The `SessionEnded` event, then the close; the message's ID.
fn session_ended(r: &mut Run) -> String {
    let got = r.client.recv().expect("SessionEnded");
    assert_eq!((got.kind, got.request_id, got.service, got.op), (Kind::Event, 0, Service::SignOn, event::SESSION_ENDED));
    assert_eq!(got.status, Status::Ok);
    let m = got.message.expect("a message");
    assert!(m.text.ends_with("(lang of BOB)"), "{}", m.text);
    assert!(r.client.closed());
    m.id
}

#[test]
fn attach_statuses() {
    let sys = Controls::new();
    let (mut one, token) = bob(&sys);
    // Ok: the same token and profile; this connection is signed on.
    let mut two = hello(&sys, 4);
    let got = attach(&mut two, &token);
    assert_eq!(got.status, Status::Ok);
    let reply: SignOnReply = got.body_as().unwrap();
    assert_eq!((reply.token, reply.profile.as_str()), (token.clone(), "BOB"));
    assert_eq!(two.client.call(Op::SystemInfo, &()).status, Status::Ok);
    assert_eq!(one.client.call(Op::SystemInfo, &()).status, Status::Ok, "the session stays with the first");
    let running = sys.running_as.lock().unwrap().clone();
    assert_eq!((running.len(), running.values().all(|u| u == "BOB")), (2, true));
    // On one signed-on connection: refused, the connection and session stay.
    assert_eq!(attach(&mut two, &token).status, Status::InvalidRequest);
    assert_eq!(attach(&mut one, &token).status, Status::InvalidRequest);
    assert_eq!(two.client.call(Op::SystemInfo, &()).status, Status::Ok);
    // An unknown token.
    let mut three = hello(&sys, 4);
    assert_eq!(status_msg(&attach(&mut three, &Token([0xEE; 32]))), (Status::SessionExpired, "TCP1A53".into()));
    assert_eq!(three.client.call(Op::SystemInfo, &()).status, Status::NotSignedOn);
    // Not the first request after Hello: not after one failed one, nor
    // after one Ping.
    assert_eq!(attach(&mut three, &token).status, Status::InvalidRequest);
    let mut pinged = hello(&sys, 4);
    assert_eq!(pinged.client.call(Op::Ping, &()).status, Status::Ok);
    assert_eq!(attach(&mut pinged, &token).status, Status::InvalidRequest);
    assert_eq!(pinged.client.call(Op::SystemInfo, &()).status, Status::NotSignedOn);
    // A version 3 connection: no such operation.
    let mut v3 = hello(&sys, 3);
    assert_eq!(attach(&mut v3, &token).status, Status::InvalidRequest);
    assert_eq!(v3.client.call(Op::SystemInfo, &()).status, Status::NotSignedOn);
}

#[test]
fn an_expired_session_does_not_attach() {
    let sys = Controls::new();
    let (one, token) = bob(&sys);
    close(one);
    sys.advance(600);
    let mut two = hello(&sys, 4);
    assert_eq!(status_msg(&attach(&mut two, &token)), (Status::SessionExpired, "TCP1A53".into()));
}

#[test]
fn each_connection_has_its_own_job_state() {
    let sys = Controls::new();
    let (mut one, token) = bob(&sys);
    let mut two = attached(&sys, &token);
    assert_eq!(run(&mut one, "CRTLIB LIB(MINE)").0, Status::Ok);
    let (s, on_a) = run(&mut one, "CHGCURLIB CURLIB(MINE)");
    assert_eq!(s, Status::Ok);
    let (s, on_b) = run(&mut two, "CRTLIB LIB(THEIRS)");
    assert_eq!(s, Status::Ok);
    let (ja, jb) = (job_number(&on_a), job_number(&on_b));
    assert_ne!(ja, jb, "a job each");
    let curlib = sys.curlib.lock().unwrap().clone();
    assert_eq!(curlib.get(&ja).map(String::as_str), Some("MINE"));
    assert_eq!(curlib.get(&jb), None, "the second's current library is its own");
    assert_eq!(run(&mut two, "CHGCURLIB CURLIB(THEIRS)").0, Status::Ok);
    let curlib = sys.curlib.lock().unwrap().clone();
    assert_eq!((curlib[&ja].as_str(), curlib[&jb].as_str()), ("MINE", "THEIRS"));
    // Both run as the session's profile.
    let running = sys.running_as.lock().unwrap().clone();
    assert_eq!((running[&ja].as_str(), running[&jb].as_str()), ("BOB", "BOB"));
}

#[test]
fn sixteen_connections_at_most() {
    let sys = Controls::new();
    let (_a, token) = bob(&sys);
    let mut more: Vec<Run> = (0..15).map(|_| attached(&sys, &token)).collect();
    let mut full = hello(&sys, 4);
    let got = attach(&mut full, &token);
    assert_eq!(status_msg(&got), (Status::Busy, "TCP1A59".into()));
    assert!(got.message.unwrap().text.contains("BOB"));
    assert_eq!(full.client.call(Op::SystemInfo, &()).status, Status::NotSignedOn);
    // One closes: one more fits.
    close(more.pop().unwrap());
    let mut again = hello(&sys, 4);
    assert_eq!(attach(&mut again, &token).status, Status::Ok);
    let mut over = hello(&sys, 4);
    assert_eq!(attach(&mut over, &token).status, Status::Busy);
}

#[test]
fn sign_off_on_one_ends_the_others() {
    let sys = Controls::new();
    // The connection that signs on speaks version 1: what it is sent is
    // version 1's.
    let mut one = hello(&sys, 1);
    let s = one.client.call(
        Op::SignOn,
        &SignOn {
            user: "BOB".into(),
            password: Secret::from("SECRET1"),
        },
    );
    let token = s.body_as::<SignOnReply>().unwrap().token;
    let mut two = attached(&sys, &token);
    let mut three = attached(&sys, &token);
    // The second signs off: its Ok, and it stays open, not signed on.
    let off = two.client.call(Op::SignOff, &());
    assert_eq!((off.status, off.body.len()), (Status::Ok, 0));
    assert_eq!(two.client.call(Op::SystemInfo, &()).status, Status::NotSignedOn);
    assert_eq!(two.client.call(Op::Ping, &()).status, Status::Ok);
    // The others: SessionEnded, then closed.
    assert_eq!(session_ended(&mut one), "TCP1A58");
    assert_eq!(session_ended(&mut three), "TCP1A58");
    assert_eq!(one.server.join().unwrap(), Closed::SessionEnded);
    assert_eq!(three.server.join().unwrap(), Closed::SessionEnded);
    // The token is gone.
    let mut four = hello(&sys, 4);
    assert_eq!(four.client.call(Op::Resume, &Resume { token: token.clone() }).status, Status::SessionExpired);
    let mut five = hello(&sys, 4);
    assert_eq!(attach(&mut five, &token).status, Status::SessionExpired);
}

#[test]
fn the_idle_timer_runs_only_without_connections() {
    let sys = Controls::new();
    let (one, token) = bob(&sys);
    let two = attached(&sys, &token);
    // The connection that signed on goes; the attached one keeps the
    // session as long as it stays.
    close(one);
    sys.advance(10_000);
    let three = attached(&sys, &token);
    close(two);
    sys.advance(10_000);
    close(three);
    // Now none: the timeout (600 s) runs from here.
    sys.advance(599);
    let mut four = hello(&sys, 4);
    assert_eq!(attach(&mut four, &token).status, Status::Ok);
    close(four);
    sys.advance(600);
    let mut five = hello(&sys, 4);
    assert_eq!(attach(&mut five, &token).status, Status::SessionExpired);
}

#[test]
fn resume_moves_only_the_resumable_connection() {
    let sys = Controls::new();
    let (one, token) = bob(&sys);
    let mut two = attached(&sys, &token);
    close(one);
    // Resume takes the place of the connection that signed on.
    let mut three = hello(&sys, 4);
    assert_eq!(three.client.call(Op::Resume, &Resume { token: token.clone() }).status, Status::Ok);
    assert_eq!(two.client.call(Op::SystemInfo, &()).status, Status::Ok, "the second stays attached");
    // Resume with the session's connection still open moves it; the second stays.
    let mut four = hello(&sys, 4);
    assert_eq!(four.client.call(Op::Resume, &Resume { token: token.clone() }).status, Status::Ok);
    assert_eq!(two.client.call(Op::SystemInfo, &()).status, Status::Ok);
    // The second signs off: the fourth, the resumable one now, ends.
    assert_eq!(two.client.call(Op::SignOff, &()).status, Status::Ok);
    assert_eq!(session_ended(&mut four), "TCP1A58");
    assert_eq!(four.server.join().unwrap(), Closed::SessionEnded);
}

#[test]
fn an_operator_ending_one_job_ends_the_session() {
    let sys = Controls::new();
    let (mut one, token) = bob(&sys);
    let mut two = attached(&sys, &token);
    // Whichever job the operator ends, both connections hear TCP1A54.
    sys.end(Ending::Operator);
    assert_eq!(session_ended(&mut one), "TCP1A54");
    assert_eq!(session_ended(&mut two), "TCP1A54");
    let mut why = [one.server.join().unwrap(), two.server.join().unwrap()];
    why.sort_by_key(|w| format!("{w:?}"));
    assert_eq!(why, [Closed::Operator, Closed::SessionEnded]);
    let mut three = hello(&sys, 4);
    assert_eq!(attach(&mut three, &token).status, Status::SessionExpired);
}
