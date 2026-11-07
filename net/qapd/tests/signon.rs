// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `SignOn` and `System` services of docs/QAP.md through a client built
//! from `qap-proto`: a good sign-on, bad passwords up to `QMAXSIGN`,
//! disabled and `*NONE` profiles, an expired password then a change,
//! resume within and after the idle timeout, sign-off, `SessionEnded`,
//! messages in the user's language, and a reply over 1 MiB split with
//! `More`.

mod common;

use std::sync::Arc;

use qap_proto::envelope::MORE;
use qap_proto::ops::event;
use qap_proto::ops::signon::{ChangePassword, Resume, SignOn, SignOnReply};
use qap_proto::ops::system::SystemInfo;
use qap_proto::{Envelope, Kind, MAX_FRAME, Op, Secret, Service, Status, Token};
use qapd::{Closed, Ending};

use common::{Controls, Run, cfg, start_with};

fn sign_on(r: &mut Run, user: &str, password: &str) -> Envelope {
    r.client.call(
        Op::SignOn,
        &SignOn {
            user: user.into(),
            password: Secret::from(password),
        },
    )
}

fn change(r: &mut Run, user: &str, old: &str, new: &str) -> Envelope {
    r.client.call(
        Op::ChangePassword,
        &ChangePassword {
            user: user.into(),
            old_password: Secret::from(old),
            new_password: Secret::from(new),
        },
    )
}

fn resume(r: &mut Run, token: &Token) -> Envelope {
    r.client.call(
        Op::Resume,
        &Resume {
            token: token.clone(),
        },
    )
}

fn hello(c: &Arc<Controls>) -> Run {
    let mut r = start_with(cfg(), Arc::clone(c));
    r.client.hello();
    r
}

/// The message's ID, after checking the status.
fn refused(e: &Envelope, status: Status) -> String {
    assert_eq!(e.status, status, "{e:?}");
    assert!(e.body.is_empty(), "a refusal carries no token");
    e.message.as_ref().expect("a message").id.clone()
}

#[test]
fn good_sign_on() {
    let c = Controls::new();
    let mut r = hello(&c);
    assert_eq!(
        r.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
    let s = sign_on(&mut r, "bob", "SECRET1");
    assert_eq!(s.status, Status::Ok);
    let reply: SignOnReply = s.body_as().unwrap();
    assert_eq!(reply.profile, "BOB");
    assert_eq!(reply.idle_timeout_secs, 600);
    assert_eq!(reply.home_directory, "/home/BOB");
    assert_eq!(c.running_as.lock().unwrap().values().next().unwrap(), "BOB");
    // Signed on: the System service answers, a second sign-on does not.
    let info = r.client.call(Op::SystemInfo, &());
    assert_eq!(info.status, Status::Ok);
    let info: SystemInfo = info.body_as().unwrap();
    assert_eq!(
        (info.system_name.as_str(), info.products.len()),
        ("QSHQ400", 3)
    );
    assert_eq!(
        sign_on(&mut r, "BOB", "SECRET1").status,
        Status::InvalidRequest
    );
    assert_eq!(resume(&mut r, &reply.token).status, Status::InvalidRequest);
}

#[test]
fn bad_passwords_up_to_qmaxsign() {
    let c = Controls::new();
    let mut r = hello(&c);
    assert_eq!(
        refused(&sign_on(&mut r, "BOB", "wrong"), Status::SignOnFailed),
        "CPF1107"
    );
    assert_eq!(
        refused(&sign_on(&mut r, "BOB", "wrong"), Status::SignOnFailed),
        "CPF1392"
    );
    assert_eq!(
        refused(&sign_on(&mut r, "BOB", "wrong"), Status::SignOnFailed),
        "CPF1393"
    );
    assert!(c.get("BOB").disabled);
    // Disabled now: the right password does not help.
    assert_eq!(
        refused(&sign_on(&mut r, "BOB", "SECRET1"), Status::SignOnFailed),
        "CPF1394"
    );
    assert_eq!(
        *c.audit.lock().unwrap(),
        ["PW P BOB", "PW P BOB", "PW P BOB", "PW Q BOB"]
    );
    // An unknown user reads like a wrong password.
    assert_eq!(
        refused(&sign_on(&mut r, "NOBODY", "x"), Status::SignOnFailed),
        "CPF1107"
    );
    assert_eq!(c.audit.lock().unwrap().last().unwrap(), "PW U NOBODY");
    // Still not signed on.
    assert_eq!(
        r.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
}

#[test]
fn disabled_and_none_profiles() {
    let c = Controls::new();
    c.user("ANN", "PASSWD1");
    c.edit("ANN", |u| u.disabled = true);
    c.user("SVC", "");
    c.edit("SVC", |u| u.none = true);
    let mut r = hello(&c);
    let e = sign_on(&mut r, "ANN", "PASSWD1");
    assert_eq!(refused(&e, Status::SignOnFailed), "CPF1394");
    assert_eq!(e.message.unwrap().data, b"ANN");
    assert_eq!(
        refused(&sign_on(&mut r, "SVC", ""), Status::SignOnFailed),
        "CPF1118"
    );
}

#[test]
fn expired_password_then_change() {
    let c = Controls::new();
    c.edit("BOB", |u| u.expired = true);
    let mut r = hello(&c);
    assert_eq!(
        refused(&sign_on(&mut r, "BOB", "SECRET1"), Status::PasswordExpired),
        "CPD9BAE"
    );
    assert_eq!(
        r.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
    // The system's rules refuse a new password; the old one is checked
    // as a sign-on is.
    assert_eq!(
        refused(&change(&mut r, "BOB", "SECRET1", "abc"), Status::Error),
        "CPF2362"
    );
    assert_eq!(
        refused(
            &change(&mut r, "BOB", "nope", "NEWPASS1"),
            Status::SignOnFailed
        ),
        "CPF1107"
    );
    let ok = change(&mut r, "BOB", "SECRET1", "NEWPASS1");
    assert_eq!(ok.status, Status::Ok);
    let reply: SignOnReply = ok.body_as().unwrap();
    assert_eq!(reply.profile, "BOB");
    assert_ne!(reply.token, Token([0; 32]));
    assert_eq!(r.client.call(Op::SystemInfo, &()).status, Status::Ok);
}

#[test]
fn change_password_when_signed_on() {
    let c = Controls::new();
    c.user("ANN", "PASSWD1");
    let mut r = hello(&c);
    let token = sign_on(&mut r, "BOB", "SECRET1")
        .body_as::<SignOnReply>()
        .unwrap()
        .token;
    // Only the session's own profile.
    assert_eq!(
        refused(
            &change(&mut r, "ANN", "PASSWD1", "PASSWD2"),
            Status::NotAuthorized
        ),
        "CPF22E9"
    );
    let ok = change(&mut r, "BOB", "SECRET1", "SECRET2");
    assert_eq!(ok.status, Status::Ok);
    assert_eq!(
        ok.body_as::<SignOnReply>().unwrap().token,
        token,
        "the session goes on"
    );
    assert_eq!(c.get("BOB").password, "SECRET2");
}

#[test]
fn resume_within_and_after_the_timeout() {
    let c = Controls::new();
    let mut a = hello(&c);
    let token = sign_on(&mut a, "BOB", "SECRET1")
        .body_as::<SignOnReply>()
        .unwrap()
        .token;
    drop(a.client);
    assert_eq!(a.server.join().unwrap(), Closed::Peer);
    // Within the idle timeout: the same session, timeout restarted.
    c.advance(599);
    let mut b = hello(&c);
    let back = resume(&mut b, &token);
    assert_eq!(back.status, Status::Ok);
    let reply: SignOnReply = back.body_as().unwrap();
    assert_eq!(
        (reply.token.clone(), reply.profile.as_str()),
        (token.clone(), "BOB")
    );
    assert_eq!(b.client.call(Op::SystemInfo, &()).status, Status::Ok);
    drop(b.client);
    b.server.join().unwrap();
    // After it: expired.
    c.advance(600);
    let mut d = hello(&c);
    assert_eq!(
        refused(&resume(&mut d, &token), Status::SessionExpired),
        "TCP1A53"
    );
    assert_eq!(
        d.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
}

#[test]
fn sign_off_ends_the_session() {
    let c = Controls::new();
    let mut r = hello(&c);
    let token = sign_on(&mut r, "BOB", "SECRET1")
        .body_as::<SignOnReply>()
        .unwrap()
        .token;
    let off = r.client.call(Op::SignOff, &());
    assert_eq!((off.status, off.body.len()), (Status::Ok, 0));
    assert!(
        c.running_as.lock().unwrap().is_empty(),
        "the job runs as QUSER again"
    );
    assert_eq!(
        r.client.call(Op::SystemInfo, &()).status,
        Status::NotSignedOn
    );
    assert_eq!(r.client.call(Op::SignOff, &()).status, Status::NotSignedOn);
    assert_eq!(
        refused(&resume(&mut r, &token), Status::SessionExpired),
        "TCP1A53"
    );
}

#[test]
fn an_operator_ends_the_session() {
    let c = Controls::new();
    let mut r = hello(&c);
    let token = sign_on(&mut r, "BOB", "SECRET1")
        .body_as::<SignOnReply>()
        .unwrap()
        .token;
    *c.ending.lock().unwrap() = Some(Ending::Operator);
    let e = r.client.recv().unwrap();
    assert_eq!(
        (e.kind, e.request_id, e.service, e.op),
        (Kind::Event, 0, Service::SignOn, event::SESSION_ENDED)
    );
    assert_eq!(e.status, Status::Ok);
    let m = e.message.unwrap();
    assert_eq!(m.id, "TCP1A54");
    // In the user's language: looked up before the job left the profile.
    assert!(m.text.ends_with("(lang of BOB)"), "{}", m.text);
    assert!(r.client.closed());
    assert_eq!(r.server.join().unwrap(), Closed::Operator);
    let mut again = hello(&c);
    assert_eq!(
        refused(&resume(&mut again, &token), Status::SessionExpired),
        "TCP1A53"
    );
}

#[test]
fn messages_before_sign_on_are_the_systems() {
    let c = Controls::new();
    let mut r = hello(&c);
    let m = sign_on(&mut r, "BOB", "wrong").message.unwrap();
    assert!(m.text.ends_with("(system)"), "{}", m.text);
}

#[test]
fn a_reply_over_1_mib_is_split_with_more() {
    let c = Controls::new();
    *c.products.lock().unwrap() = 60_000;
    let mut r = hello(&c);
    sign_on(&mut r, "BOB", "SECRET1");
    // The frames as they come, before the client joins them.
    let id = r.client.request(Op::SystemInfo, &());
    let mut frames = Vec::new();
    loop {
        let f = r.client.recv_frame().expect("a frame");
        assert_eq!((f.kind, f.request_id), (Kind::Reply, id));
        assert!(ipc::encode(&f).len() <= MAX_FRAME);
        let last = f.flags & MORE == 0;
        frames.push(f);
        if last {
            break;
        }
    }
    assert!(frames.len() >= 2, "{} frames", frames.len());
    assert!(frames.iter().all(|f| f.status == Status::Ok));
    let body: Vec<u8> = frames.iter().flat_map(|f| f.body.iter().copied()).collect();
    assert!(body.len() > MAX_FRAME);
    let info: SystemInfo = ipc::decode(&body).unwrap();
    assert_eq!(info.products.len(), 60_000);
    assert_eq!(info.products[59_999].id, "QS59999");
    // The connection goes on.
    assert_eq!(r.client.call(Op::Ping, &()).status, Status::Ok);
}

#[test]
fn passwords_and_tokens_never_print() {
    let s = SignOn {
        user: "BOB".into(),
        password: Secret::from("SECRET1"),
    };
    let t = Resume {
        token: Token([7; 32]),
    };
    let printed = format!("{s:?} {t:?}");
    assert!(
        !printed.contains("SECRET1") && !printed.contains("7, 7"),
        "{printed}"
    );
}
