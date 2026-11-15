// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message of every family (`qsi::samples`) through a console
//! session and a host session, in pieces of several sizes: each request
//! arrives as sent, each reply comes back as sent, each event is pushed
//! in order.

use qsi::any::AnyEvent;
use qsi::{base, samples};
use qsisess::loopback::Loopback;
use qsisess::{EventLog, Incoming, Limits};

#[test]
fn every_request_and_reply_of_every_family() {
    let exchanges = samples::exchanges();
    assert!(exchanges.len() >= 60, "{} exchanges", exchanges.len());
    for piece in [1, 13, 4096] {
        let mut l = Loopback::new(true);
        l.piece = piece;
        for (req, reply) in &exchanges {
            let (got_req, got_reply) = l.round_trip(req, reply);
            assert_eq!(&got_req, req);
            assert_eq!(&got_reply, reply);
        }
        assert!(l.console.is_open() && l.managed.is_open());
    }
}

#[test]
fn every_event_of_every_family() {
    let events = samples::events();
    let mut l = Loopback::new(true);
    let limits = Limits::default();
    let mut log = EventLog::new(&limits);
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
    assert_eq!(l.managed.publish(&log).unwrap(), events.len());
    l.pump();
    let got: Vec<(u64, AnyEvent)> = l
        .at_console
        .iter()
        .filter_map(|i| match i {
            Incoming::Event(e) => Some((e.seq, e.decode().unwrap().unwrap())),
            _ => None,
        })
        .collect();
    assert_eq!(
        got.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>(),
        events
    );
    let seqs: Vec<u64> = got.iter().map(|(s, _)| *s).collect();
    assert_eq!(seqs, (1..=events.len() as u64).collect::<Vec<_>>());
}
