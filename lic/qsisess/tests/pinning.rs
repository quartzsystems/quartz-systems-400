// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A managed session whose console is not pinned: pairing only.

mod common;

use common::refused;
use qsi::any::{AnyReply, AnyRequest};
use qsi::{base, msgid, pair};
use qsisess::loopback::{Loopback, console_hello, host_hello};
use qsisess::{Error, EventLog, Incoming, Limits, Role, Session};

#[test]
fn pairing_only() {
    let mut l = Loopback::new(false);
    assert!(!l.managed.pinned());
    // Pairing passes.
    let start = AnyRequest::Pair(pair::Request::Start(pair::Start {
        spake: vec![9; 33],
        name: "MC01".into(),
    }));
    let answer = AnyReply::Pair(pair::Reply::Answer(pair::Answer {
        spake: vec![8; 33],
        confirm: vec![7; 32],
        name: "VS01".into(),
    }));
    assert_eq!(l.round_trip(&start, &answer), (start, answer));
    // Anything else is refused by the session; the caller sees nothing.
    let id = l
        .console
        .request_msg("U", &base::Request::Identify(qsi::Empty))
        .unwrap();
    let sub = l
        .console
        .request_msg(
            "U",
            &base::Request::Subscribe(base::Subscribe { since: None }),
        )
        .unwrap();
    l.pump();
    assert!(
        !l.at_managed
            .iter()
            .any(|i| matches!(i, Incoming::Request(_)))
    );
    assert_eq!(
        refused(&l.at_console, id).as_deref(),
        Some(msgid::NOT_PINNED)
    );
    assert_eq!(
        refused(&l.at_console, sub).as_deref(),
        Some(msgid::NOT_PINNED)
    );
    // No events, no streams.
    let mut log = EventLog::new(&Limits::default());
    log.push(qsi::family::BASE, base::event::STATE, Vec::new())
        .unwrap();
    assert_eq!(l.managed.publish(&log).unwrap(), 0);
    let id = l
        .console
        .request_msg(
            "U",
            &pair::Request::Confirm(pair::Confirm { confirm: vec![1] }),
        )
        .unwrap();
    l.pump();
    assert_eq!(
        l.managed.subscribe(id, &log, None).unwrap_err(),
        Error::NotPinned
    );
    assert_eq!(
        l.managed.reply_stream(id, Vec::new()).unwrap_err(),
        Error::NotPinned
    );
    l.managed
        .reply_msg(id, &pair::Reply::Done(qsi::Empty))
        .unwrap();
    l.pump();
    assert!(l.take_reply(id).unwrap().refusal().is_none());
}

#[test]
fn an_unpinned_console_asks_only_to_pair() {
    let mut s = Session::new(Role::Console, console_hello(), false, Limits::default(), 0).unwrap();
    let mut m = Session::new(Role::Managed, host_hello(), true, Limits::default(), 0).unwrap();
    s.feed(&m.take_output(), 0);
    let e = s
        .request_msg("U", &base::Request::Identify(qsi::Empty))
        .unwrap_err();
    assert_eq!(
        (e.clone(), e.msgid()),
        (Error::NotPinned, msgid::NOT_PINNED)
    );
    assert!(
        s.request_msg(
            "U",
            &pair::Request::Confirm(pair::Confirm { confirm: vec![] })
        )
        .is_ok()
    );
}
