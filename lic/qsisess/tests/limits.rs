// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The limits, each refusing by name; keep-alive; the clean close.

mod common;

use common::{closed, refused, replies, request};
use qsi::{base, family, msgid};
use qsisess::loopback::{Loopback, console_hello, host_hello};
use qsisess::{Error, Incoming, Limits};

fn time() -> base::Request {
    base::Request::Time(qsi::Empty)
}

fn pair(console: Limits, managed: Limits) -> Loopback {
    Loopback::with(console_hello(), host_hello(), true, console, managed).unwrap()
}

#[test]
fn too_many_requests() {
    let mut l = Loopback::new(true);
    for _ in 0..64 {
        l.console.request_msg("U", &time()).unwrap();
    }
    let e = l.console.request_msg("U", &time()).unwrap_err();
    assert_eq!(e, Error::TooManyRequests { limit: 64 });
    assert_eq!(e.msgid(), msgid::TOO_MANY_REQUESTS);

    // The managed side's own limit, against a console with a larger one.
    let mut l = pair(
        Limits::default(),
        Limits {
            max_requests: 4,
            ..Limits::default()
        },
    );
    let ids: Vec<u32> = (0..6)
        .map(|_| l.console.request_msg("U", &time()).unwrap())
        .collect();
    l.pump();
    let passed = l
        .at_managed
        .iter()
        .filter(|i| matches!(i, Incoming::Request(_)))
        .count();
    assert_eq!(passed, 4);
    for id in &ids[4..] {
        assert_eq!(
            refused(&l.at_console, *id).as_deref(),
            Some(msgid::TOO_MANY_REQUESTS)
        );
    }
}

#[test]
fn too_many_streams() {
    let mut l = pair(
        Limits::default(),
        Limits {
            max_streams: 2,
            ..Limits::default()
        },
    );
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(l.console.request_msg("U", &time()).unwrap());
    }
    l.pump();
    assert_eq!(l.managed.reply_stream(ids[0], Vec::new()).unwrap(), 1);
    assert_eq!(l.managed.reply_stream(ids[1], Vec::new()).unwrap(), 2);
    let e = l.managed.reply_stream(ids[2], Vec::new()).unwrap_err();
    assert_eq!(e, Error::TooManyStreams { limit: 2 });
    l.pump();
    assert_eq!(
        refused(&l.at_console, ids[2]).as_deref(),
        Some(msgid::TOO_MANY_STREAMS)
    );
    assert_eq!(l.console.open_streams(), 2);
}

#[test]
fn long_messages_in_parts() {
    let mut l = Loopback::new(true);
    l.piece = 5000;
    let body: Vec<u8> = (0..600_000u32).map(|i| i.to_le_bytes()[0]).collect();
    let id = l
        .console
        .request("U", family::BASE, 0x00FE, body.clone())
        .unwrap();
    l.pump();
    let r = l.take_request(id).unwrap();
    assert_eq!(r.body, body);
    l.managed.reply(id, body.clone()).unwrap();
    l.pump();
    let reply = l.take_reply(id).unwrap();
    assert_eq!(reply.outcome, qsisess::Outcome::Result { body });
}

#[test]
fn too_long() {
    let mut l = Loopback::new(true);
    let big = vec![0u8; 2 * 1024 * 1024];
    let e = l
        .console
        .request("U", family::BASE, 1, big.clone())
        .unwrap_err();
    assert!(matches!(e, Error::TooLong { .. }));
    assert_eq!(e.msgid(), msgid::TOO_LONG);
    assert_eq!(l.console.outstanding(), 0);
    // A reply too long goes as a refusal.
    let id = l.console.request_msg("U", &time()).unwrap();
    l.pump();
    assert!(matches!(
        l.managed.reply(id, big),
        Err(Error::TooLong { .. })
    ));
    l.pump();
    assert_eq!(refused(&l.at_console, id).as_deref(), Some(msgid::TOO_LONG));

    // A peer's message longer than this side's limit closes the session.
    let mut l = pair(
        Limits::default(),
        Limits {
            max_message: 64 * 1024,
            ..Limits::default()
        },
    );
    let id = l
        .console
        .request("U", family::BASE, 1, vec![0; 100_000])
        .unwrap();
    l.pump();
    assert_eq!(
        closed(&l.at_managed),
        Some((Some(msgid::TOO_LONG.into()), false))
    );
    assert_eq!(
        closed(&l.at_console),
        Some((Some(msgid::TOO_LONG.into()), true))
    );
    // Its outstanding request is answered, not left waiting.
    assert_eq!(refused(&l.at_console, id).as_deref(), Some(msgid::TOO_LONG));
}

#[test]
fn user_too_long() {
    let mut l = Loopback::new(true);
    let long = "U".repeat(129);
    let e = l.console.request_msg(&long, &time()).unwrap_err();
    assert_eq!(
        e,
        Error::UserTooLong {
            len: 129,
            limit: 128
        }
    );
    // A peer that sends one anyway is refused.
    l.managed
        .feed(&request(77, &long, family::BASE, 7, Vec::new()), 0);
    let r = replies(&l.managed.take_output());
    assert_eq!(r[0].id, 77);
    assert_eq!(r[0].refusal().unwrap().0, msgid::TOO_LONG);
    // A request number already outstanding closes the session.
    let id = l.console.request_msg("U", &time()).unwrap();
    l.pump();
    l.managed
        .feed(&request(id, "U", family::BASE, 7, Vec::new()), 0);
    assert!(l.managed.is_closed());
}

#[test]
fn keep_alive_and_timeout() {
    let mut l = Loopback::new(true);
    l.advance(14_999);
    assert!(!l.console.has_output() && !l.managed.has_output());
    l.advance(1);
    assert!(l.console.has_output() && l.managed.has_output());
    l.pump();
    // Pings answered: the clock restarts at both.
    l.advance(30_000);
    assert!(l.console.is_open());
    l.pump();
    // Nothing more delivered: the pings go unheard and both time out.
    l.advance(15_000);
    let _ = (l.console.take_output(), l.managed.take_output());
    l.advance(45_000);
    assert_eq!(
        closed(&l.at_console),
        Some((Some(msgid::TIMED_OUT.into()), false))
    );
    assert_eq!(
        closed(&l.at_managed),
        Some((Some(msgid::TIMED_OUT.into()), false))
    );
}

#[test]
fn clean_close() {
    let mut l = Loopback::new(true);
    let id = l.console.request_msg("U", &time()).unwrap();
    l.pump();
    l.console.close("done");
    l.pump();
    assert_eq!(refused(&l.at_console, id).as_deref(), Some(msgid::CLOSING));
    assert_eq!(closed(&l.at_console), Some((None, false)));
    let e = l.console.request_msg("U", &time()).unwrap_err();
    assert_eq!((e.clone(), e.msgid()), (Error::Closed, msgid::CLOSING));
    assert_eq!(closed(&l.at_managed), Some((None, true)));
    assert_eq!(l.managed.reply(id, Vec::new()).unwrap_err(), Error::Closed);

    // The managed side closes with requests outstanding.
    let mut l = Loopback::new(true);
    let id = l.console.request_msg("U", &time()).unwrap();
    l.pump();
    l.managed.close("going down");
    l.pump();
    assert_eq!(refused(&l.at_console, id).as_deref(), Some(msgid::CLOSING));
    assert!(l.at_console.iter().any(|i| matches!(i,
        Incoming::Closed { msgid: None, by_peer: true, text } if text == "going down")));
}
