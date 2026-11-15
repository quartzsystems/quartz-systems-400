// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message of the base and pair families through two sessions,
//! refusals and unknown operations.

mod common;

use common::refused;
use qsi::any::{AnyEvent, AnyReply, AnyRequest};
use qsi::base::{self, PowerAction, PowerState, Refcode, Replay};
use qsi::{Empty, PeerKind, family, msgid, pair};
use qsisess::loopback::Loopback;
use qsisess::{EventLog, Incoming, Limits};

fn refcode(seq: u64) -> Refcode {
    Refcode {
        seq,
        code: "B2001234".into(),
        words: vec!["00000001".into(), "DEADBEEF".into()],
        text: "an IPL step".into(),
        time_ms: 1_700_000_000_000,
    }
}

fn state() -> base::State {
    base::State {
        power: PowerState::Running,
        refcode: Some(refcode(7)),
        release: "V1R2M0".into(),
        slot: "A".into(),
        keylock: Some(base::Keylock::Manual),
    }
}

fn identity() -> base::Identity {
    base::Identity {
        kind: PeerKind::Host,
        name: "VS01".into(),
        release: "V1R2M0".into(),
        serial: "S0000001".into(),
        model: "9406-QSV".into(),
        build: "abc123".into(),
    }
}

/// Every request of the two families with an answer to it.
fn pairs() -> Vec<(AnyRequest, AnyReply)> {
    use base::{Reply as BR, Request as BQ};
    use pair::{Reply as PR, Request as PQ};
    vec![
        (
            BQ::Identify(Empty).into_any(),
            BR::Identify(identity()).into_any(),
        ),
        (BQ::State(Empty).into_any(), BR::State(state()).into_any()),
        (
            BQ::Power(base::Power {
                action: PowerAction::Restart,
                immediate: false,
                delay: 600,
            })
            .into_any(),
            BR::Power(Empty).into_any(),
        ),
        (
            BQ::Refcodes(base::Refcodes {
                before: Some(9),
                limit: 2,
            })
            .into_any(),
            BR::Refcodes(base::RefcodeList {
                codes: vec![refcode(8), refcode(7)],
                more: Some(7),
            })
            .into_any(),
        ),
        (
            BQ::Subscribe(base::Subscribe { since: Some(4) }).into_any(),
            BR::Subscribe(base::Subscribed {
                replay: Replay::Resumed,
                replayed: 2,
                next: 7,
            })
            .into_any(),
        ),
        (
            BQ::Unsubscribe(Empty).into_any(),
            BR::Unsubscribe(Empty).into_any(),
        ),
        (
            BQ::Time(Empty).into_any(),
            BR::Time(base::Time {
                unix_ns: -5,
                offset_s: -18_000,
                zone: "America/Chicago".into(),
            })
            .into_any(),
        ),
        (
            PQ::Start(pair::Start {
                spake: vec![1; 33],
                name: "MC01".into(),
            })
            .into_any(),
            PR::Answer(pair::Answer {
                spake: vec![2; 33],
                confirm: vec![3; 32],
                name: "VS01".into(),
            })
            .into_any(),
        ),
        (
            PQ::Confirm(pair::Confirm {
                confirm: vec![4; 32],
            })
            .into_any(),
            PR::Done(Empty).into_any(),
        ),
    ]
}

trait IntoAny<T> {
    fn into_any(self) -> T;
}
impl IntoAny<AnyRequest> for base::Request {
    fn into_any(self) -> AnyRequest {
        AnyRequest::Base(self)
    }
}
impl IntoAny<AnyReply> for base::Reply {
    fn into_any(self) -> AnyReply {
        AnyReply::Base(self)
    }
}
impl IntoAny<AnyRequest> for pair::Request {
    fn into_any(self) -> AnyRequest {
        AnyRequest::Pair(self)
    }
}
impl IntoAny<AnyReply> for pair::Reply {
    fn into_any(self) -> AnyReply {
        AnyReply::Pair(self)
    }
}

#[test]
fn every_request_and_reply() {
    for piece in [1, 7, 4096] {
        let mut l = Loopback::new(true);
        l.piece = piece;
        for (req, reply) in pairs() {
            let (got_req, got_reply) = l.round_trip(&req, &reply);
            assert_eq!(got_req, req);
            assert_eq!(got_reply, reply);
        }
        assert_eq!(l.console.outstanding(), 0);
        assert_eq!(l.managed.outstanding(), 0);
    }
}

#[test]
fn every_event() {
    let mut l = Loopback::new(true);
    let limits = Limits::default();
    let mut log = EventLog::new(&limits);
    let events = [
        AnyEvent::Base(base::Event::Refcode(refcode(1))),
        AnyEvent::Base(base::Event::State(state())),
        AnyEvent::Base(base::Event::Identity(identity())),
    ];
    let id = l
        .console
        .request_msg(
            "U",
            &base::Request::Subscribe(base::Subscribe { since: None }),
        )
        .unwrap();
    l.pump();
    l.take_request(id).unwrap();
    l.managed.subscribe(id, &log, None).unwrap();
    for e in &events {
        log.push_any(e).unwrap();
    }
    assert_eq!(l.managed.publish(&log).unwrap(), 3);
    l.pump();
    let got: Vec<AnyEvent> = l
        .at_console
        .iter()
        .filter_map(|i| match i {
            Incoming::Event(e) => Some(e.decode().unwrap().unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(got, events);
}

#[test]
fn refusals_and_the_user() {
    let mut l = Loopback::new(true);
    let id = l
        .console
        .request_msg("QSECOFR", &base::Request::Identify(Empty))
        .unwrap();
    l.pump();
    let r = l.take_request(id).unwrap();
    assert_eq!(r.user, "QSECOFR");
    assert_eq!(r.family, family::BASE);
    assert_eq!(
        r.typed::<base::Request>().unwrap(),
        Some(base::Request::Identify(Empty))
    );
    assert_eq!(r.typed::<pair::Request>().unwrap(), None);
    l.managed.refuse(id, "QSI0301", "the host is busy").unwrap();
    l.pump();
    let reply = l.take_reply(id).unwrap();
    assert_eq!(reply.refusal(), Some(("QSI0301", "the host is busy")));
    assert_eq!(reply.decode().unwrap(), None);
    // A reply of another operation than the request's is refused here.
    let id = l
        .console
        .request_msg("U", &base::Request::Identify(Empty))
        .unwrap();
    l.pump();
    assert!(l.managed.reply_msg(id, &base::Reply::Power(Empty)).is_err());
    assert!(l.managed.reply(999, Vec::new()).is_err());
}

#[test]
fn unknown_operations_and_families() {
    let mut l = Loopback::new(true);
    let op = l
        .console
        .request("U", family::BASE, 0x00FF, b"x".to_vec())
        .unwrap();
    let fam = l.console.request("U", 0x7777, 1, Vec::new()).unwrap();
    l.pump();
    for id in [op, fam] {
        let r = l.take_request(id).unwrap();
        assert_eq!(r.decode().unwrap(), None);
        l.managed.refuse_unknown(id).unwrap();
    }
    // A known operation whose body does not read.
    let bad = l
        .console
        .request("U", family::BASE, base::op::POWER, vec![1])
        .unwrap();
    l.pump();
    let r = l.take_request(bad).unwrap();
    let e = r.decode().unwrap_err();
    l.managed.refuse_bad(bad, &e).unwrap();
    l.pump();
    assert_eq!(refused(&l.at_console, op).as_deref(), Some(msgid::UNKNOWN));
    assert_eq!(refused(&l.at_console, fam).as_deref(), Some(msgid::UNKNOWN));
    assert_eq!(refused(&l.at_console, bad).as_deref(), Some(msgid::BAD));
}
