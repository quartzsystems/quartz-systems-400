// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Events: subscribe, replay on reconnect, read again, the order.

use qsi::base::{self, Replay, Subscribed};
use qsisess::loopback::Loopback;
use qsisess::{EventLog, Incoming, Limits};

fn push(log: &mut EventLog, n: usize) {
    for _ in 0..n {
        let seq = log.next();
        log.push(
            qsi::family::BASE,
            base::event::STATE,
            seq.to_le_bytes().to_vec(),
        )
        .unwrap();
    }
}

/// Subscribe the console with `since`; the answer.
fn subscribe(l: &mut Loopback, log: &EventLog, since: Option<u64>) -> Subscribed {
    let id = l
        .console
        .request_msg("U", &base::Request::Subscribe(base::Subscribe { since }))
        .unwrap();
    l.pump();
    let r = l.take_request(id).unwrap();
    let Some(base::Request::Subscribe(s)) = r.typed::<base::Request>().unwrap() else {
        panic!("not a subscribe");
    };
    l.managed.subscribe(id, log, s.since).unwrap();
    l.pump();
    let reply = l.take_reply(id).unwrap();
    match reply.typed::<base::Reply>().unwrap() {
        Some(base::Reply::Subscribe(s)) => s,
        other => panic!("{other:?}"),
    }
}

/// The events' numbers the console saw, taken.
fn seen(l: &mut Loopback) -> Vec<u64> {
    let seqs = l
        .at_console
        .iter()
        .filter_map(|i| match i {
            Incoming::Event(e) => Some(e.seq),
            _ => None,
        })
        .collect();
    l.at_console.retain(|i| !matches!(i, Incoming::Event(_)));
    seqs
}

#[test]
fn fresh_then_live() {
    let mut log = EventLog::new(&Limits::default());
    push(&mut log, 2);
    let mut l = Loopback::new(true);
    // Not subscribed: nothing goes.
    assert_eq!(l.managed.publish(&log).unwrap(), 0);
    let s = subscribe(&mut l, &log, None);
    assert_eq!(
        s,
        Subscribed {
            replay: Replay::Fresh,
            replayed: 0,
            next: 3
        }
    );
    assert!(seen(&mut l).is_empty());
    push(&mut log, 2);
    l.managed.publish(&log).unwrap();
    // Twice: nothing twice.
    assert_eq!(l.managed.publish(&log).unwrap(), 0);
    l.pump();
    assert_eq!(seen(&mut l), vec![3, 4]);
    assert_eq!(l.console.last_event(), Some(4));
}

#[test]
fn reconnect_gets_what_was_missed_in_order() {
    let mut log = EventLog::new(&Limits::default());
    let mut l = Loopback::new(true);
    subscribe(&mut l, &log, None);
    push(&mut log, 3);
    l.managed.publish(&log).unwrap();
    l.pump();
    assert_eq!(seen(&mut l), vec![1, 2, 3]);
    let since = l.console.last_event();
    drop(l);
    // Missed while away.
    push(&mut log, 3);
    let mut l = Loopback::new(true);
    l.piece = 3;
    let id = l
        .console
        .request_msg("U", &base::Request::Subscribe(base::Subscribe { since }))
        .unwrap();
    l.pump();
    l.take_request(id).unwrap();
    l.managed.subscribe(id, &log, since).unwrap();
    // A live event right after: no gap, no duplicate.
    push(&mut log, 1);
    l.managed.publish(&log).unwrap();
    l.pump();
    // The reply first, then the replayed, then the live.
    let order: Vec<String> = l
        .at_console
        .iter()
        .filter_map(|i| match i {
            Incoming::Reply(r) => Some(format!("reply {:?}", r.typed::<base::Reply>().unwrap())),
            Incoming::Event(e) => Some(format!("event {}", e.seq)),
            _ => None,
        })
        .collect();
    let reply = format!(
        "reply {:?}",
        Some(base::Reply::Subscribe(Subscribed {
            replay: Replay::Resumed,
            replayed: 3,
            next: 7
        }))
    );
    assert_eq!(
        order,
        vec![
            reply,
            "event 4".into(),
            "event 5".into(),
            "event 6".into(),
            "event 7".into()
        ]
    );
}

#[test]
fn read_again_when_the_log_no_longer_reaches() {
    let limits = Limits {
        log_events: 3,
        ..Limits::default()
    };
    let mut log = EventLog::new(&limits);
    push(&mut log, 10);
    assert_eq!(log.first(), 8);
    let mut l = Loopback::new(true);
    let s = subscribe(&mut l, &log, Some(2));
    assert_eq!((s.replay, s.replayed, s.next), (Replay::ReadAgain, 0, 11));
    assert!(seen(&mut l).is_empty());
    push(&mut log, 1);
    l.managed.publish(&log).unwrap();
    l.pump();
    assert_eq!(seen(&mut l), vec![11]);

    // Just within reach: resumed.
    let mut l = Loopback::new(true);
    let s = subscribe(&mut l, &log, Some(8));
    assert_eq!((s.replay, s.replayed), (Replay::Resumed, 3));
    assert_eq!(seen(&mut l), vec![9, 10, 11]);
    // Seen everything: resumed with none.
    let mut l = Loopback::new(true);
    let s = subscribe(&mut l, &log, Some(11));
    assert_eq!((s.replay, s.replayed), (Replay::Resumed, 0));
    // Newer than the log has (another log, after a restart).
    let mut l = Loopback::new(true);
    let s = subscribe(&mut l, &log, Some(500));
    assert_eq!(s.replay, Replay::ReadAgain);
}

#[test]
fn unsubscribe_stops_them() {
    let mut log = EventLog::new(&Limits::default());
    let mut l = Loopback::new(true);
    subscribe(&mut l, &log, None);
    let id = l
        .console
        .request_msg("U", &base::Request::Unsubscribe(qsi::Empty))
        .unwrap();
    l.pump();
    l.take_request(id).unwrap();
    l.managed.unsubscribe(id).unwrap();
    assert!(!l.managed.subscribed());
    push(&mut log, 2);
    assert_eq!(l.managed.publish(&log).unwrap(), 0);
    l.pump();
    assert!(seen(&mut l).is_empty());
}

#[test]
fn a_resumed_log_numbers_on_and_replays_what_it_was_given() {
    // A managed side kept events 41 to 45 and had numbered up to 45.
    let limits = Limits::default();
    let kept: Vec<qsisess::Event> = (41..=45)
        .map(|seq: u64| qsisess::Event {
            seq,
            family: qsi::family::BASE,
            op: base::event::STATE,
            body: seq.to_le_bytes().to_vec(),
        })
        .collect();
    let mut log = EventLog::resume(&limits, 46, kept.clone()).unwrap();
    assert_eq!((log.first(), log.next(), log.len()), (41, 46, 5));
    // A console that saw 42 before the restart is given 43 to 45 as
    // they were, then the new ones from 46: no placeholder, no gap.
    let mut l = Loopback::new(true);
    let s = subscribe(&mut l, &log, Some(42));
    assert_eq!(
        s,
        Subscribed {
            replay: Replay::Resumed,
            replayed: 3,
            next: 46
        }
    );
    push(&mut log, 2);
    l.managed.publish(&log).unwrap();
    l.pump();
    let got: Vec<qsisess::Event> = l
        .at_console
        .iter()
        .filter_map(|i| match i {
            Incoming::Event(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(&got[..3], &kept[2..]);
    assert_eq!(
        got.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![43, 44, 45, 46, 47]
    );
    // One older than what was kept reads again; so does one newer than
    // anything numbered.
    let mut l = Loopback::new(true);
    assert_eq!(subscribe(&mut l, &log, Some(39)).replay, Replay::ReadAgain);
    let mut l = Loopback::new(true);
    assert_eq!(subscribe(&mut l, &log, Some(90)).replay, Replay::ReadAgain);
    // Taken up with nothing kept: empty, numbering on.
    let mut empty = EventLog::resume(&limits, 1000, Vec::new()).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.first(), 1000);
    assert_eq!(empty.push(1, 1, Vec::new()).unwrap(), 1000);
}
