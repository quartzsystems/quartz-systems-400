// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bits added since `HOST_V1` between peers of different ages:
//! problems, code update and the system's own dumps (handoff 19),
//! physical I/O and partition version 2 (handoff 20). A peer without a
//! capability bit is never asked its messages and never sent its
//! events, and the rest goes on.

mod common;

use common::{replies, request};
use qsi::any::{AnyEvent, AnyRequest};
use qsi::{Hello, base, caps, family, msgid, samples};
use qsisess::loopback::{Loopback, console_hello, host_hello};
use qsisess::{Error, EventLog, Incoming, Limits};

/// A console of before this version: the host's families, none of the
/// new bits.
fn older_console() -> Hello {
    Hello {
        caps: caps::HOST_V1,
        ..console_hello()
    }
}

/// A host of before this version.
fn older_host() -> Hello {
    Hello {
        caps: caps::HOST_V1,
        ..host_hello()
    }
}

/// Every bit added beside `HOST_V1`.
const NEW: u64 = caps::SERVICING | caps::PHYSICAL_IO | caps::PARTITION_V2;

/// Each of them alone.
const BITS: [u64; 5] = [
    caps::SERVICE,
    caps::UPDATE,
    caps::SYSTEM_DUMPS,
    caps::PHYSICAL_IO,
    caps::PARTITION_V2,
];

/// The requests that need any of `bits`.
fn requests_of(bits: u64) -> Vec<AnyRequest> {
    samples::exchanges()
        .into_iter()
        .map(|(q, _)| q)
        .filter(|q| caps::of_request(q.family(), q.op()) & bits != 0)
        .collect()
}

fn events_of(bits: u64) -> Vec<AnyEvent> {
    samples::events()
        .into_iter()
        .filter(|e| caps::of_event(e.family(), e.op()) & bits != 0)
        .collect()
}

/// The new requests.
fn new_requests() -> Vec<AnyRequest> {
    requests_of(NEW)
}

fn new_events() -> Vec<AnyEvent> {
    events_of(NEW)
}

fn pair(console: Hello, managed: Hello) -> Loopback {
    Loopback::with(console, managed, true, Limits::default(), Limits::default()).unwrap()
}

#[test]
fn the_new_messages_are_these() {
    let q = requests_of(caps::SERVICING);
    // Base's four dumps, service's three, update's six.
    assert_eq!(q.len(), 13);
    assert_eq!(q.iter().filter(|q| q.family() == family::BASE).count(), 4);
    // Base's two, service's four, update's seven.
    assert_eq!(events_of(caps::SERVICING).len(), 13);
    // The host's devices and vres's tape drives; the host's five events
    // and the drive's.
    let q = requests_of(caps::PHYSICAL_IO);
    assert_eq!(q.len(), 2);
    assert_eq!(q.iter().filter(|q| q.family() == family::HOST).count(), 1);
    assert_eq!(events_of(caps::PHYSICAL_IO).len(), 6);
    // The entitlement and its event.
    assert_eq!(requests_of(caps::PARTITION_V2).len(), 1);
    assert_eq!(events_of(caps::PARTITION_V2).len(), 1);
    assert_eq!(new_requests().len(), 16);
    assert_eq!(new_events().len(), 20);
}

#[test]
fn a_console_does_not_ask_an_older_managed_side() {
    for (console, host) in [
        (console_hello(), older_host()),
        (older_console(), host_hello()),
    ] {
        let mut l = pair(console, host);
        assert_eq!(l.console.agreed().unwrap().caps, caps::HOST_V1);
        for q in new_requests() {
            let e = l.console.request_any("U", &q).unwrap_err();
            assert!(matches!(e, Error::NotAgreed { .. }), "{q:?}: {e}");
            assert_eq!(e.msgid(), msgid::NOT_AGREED);
        }
        l.pump();
        assert!(
            l.at_managed
                .iter()
                .all(|i| !matches!(i, Incoming::Request(_)))
        );
        // What both have still goes.
        let (q, p) = samples::exchanges().into_iter().next().unwrap();
        assert_eq!(l.round_trip(&q, &p), (q, p));
    }
}

#[test]
fn a_managed_side_refuses_what_was_not_agreed() {
    // A console that sends anyway, by raw frames, past its own session.
    let mut l = pair(older_console(), host_hello());
    let mut wire = Vec::new();
    let mut ids = Vec::new();
    for (n, q) in new_requests().iter().enumerate() {
        let id = 100 + u32::try_from(n).unwrap();
        ids.push(id);
        wire.extend(request(id, "U", q.family(), q.op(), q.encode()));
    }
    l.managed.feed(&wire, l.now);
    while let Some(i) = l.managed.poll() {
        assert!(!matches!(i, Incoming::Request(_)), "handed up: {i:?}");
    }
    // Each refused by the session itself, with QSI0003, in order.
    let back = replies(&l.managed.take_output());
    assert_eq!(back.iter().map(|r| r.id).collect::<Vec<_>>(), ids);
    for r in &back {
        assert_eq!(r.refusal().map(|(m, _)| m), Some(msgid::NOT_AGREED));
    }
    assert!(l.managed.is_open());
    assert_eq!(l.managed.outstanding(), 0);
}

#[test]
fn events_go_only_where_agreed() {
    let limits = Limits::default();
    let mut log = EventLog::new(&limits);
    let mut newer = Loopback::new(true);
    let mut older = pair(older_console(), host_hello());
    for l in [&mut newer, &mut older] {
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
        l.pump();
    }
    let every = samples::events();
    for e in &every {
        log.push_any(e).unwrap();
    }
    let new = new_events().len();
    assert_eq!(newer.managed.publish(&log).unwrap(), every.len());
    assert_eq!(older.managed.publish(&log).unwrap(), every.len() - new);
    let seen = |l: &mut Loopback| -> Vec<(u64, AnyEvent)> {
        l.pump();
        l.at_console
            .iter()
            .filter_map(|i| match i {
                Incoming::Event(e) => Some((e.seq, e.decode().unwrap().unwrap())),
                _ => None,
            })
            .collect()
    };
    let to_newer = seen(&mut newer);
    let to_older = seen(&mut older);
    assert_eq!(
        to_newer.iter().map(|(_, e)| e.clone()).collect::<Vec<_>>(),
        every
    );
    for (_, e) in &to_older {
        assert_eq!(
            caps::of_event(e.family(), e.op()) & NEW,
            0,
            "{e:?}"
        );
    }
    // The older console's numbers rise and skip the ones it was not sent.
    assert!(to_older.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(to_older.windows(2).any(|w| w[1].0 > w[0].0 + 1));

    // A reconnect replays only what it may have.
    let since = Some(1);
    let mut again = pair(older_console(), host_hello());
    let id = again
        .console
        .request_msg("U", &base::Request::Subscribe(base::Subscribe { since }))
        .unwrap();
    again.pump();
    again.take_request(id).unwrap();
    again.managed.subscribe(id, &log, since).unwrap();
    again.pump();
    let reply = again.take_reply(id).unwrap();
    let Some(base::Reply::Subscribe(s)) = reply.typed::<base::Reply>().unwrap() else {
        panic!("not subscribed");
    };
    assert_eq!(s.replay, base::Replay::Resumed);
    let replayed = seen(&mut again);
    assert_eq!(replayed.len(), usize::try_from(s.replayed).unwrap());
    assert_eq!(replayed.len(), to_older.len() - 1);
}

#[test]
fn service_and_update_need_only_their_own_bits() {
    // A host that has problems but no code update, and no dumps of its
    // own: the console may ask problems and nothing else of the new.
    let host = Hello {
        caps: caps::HOST_V1 | caps::SERVICE,
        ..host_hello()
    };
    let mut l = pair(console_hello(), host);
    for q in new_requests() {
        let r = l.console.request_any("U", &q);
        if q.family() == family::SERVICE {
            assert!(r.is_ok(), "{q:?}");
        } else {
            assert!(matches!(r, Err(Error::NotAgreed { .. })), "{q:?}");
        }
    }
}

#[test]
fn each_bit_alone() {
    for bit in BITS {
        let with = Hello {
            caps: caps::HOST_V1 | bit,
            ..host_hello()
        };
        let console = Hello {
            caps: caps::HOST_V1 | bit,
            ..console_hello()
        };
        // A host with the bit alone, a newer console; then the other way.
        for (c, h) in [(console_hello(), with.clone()), (console, host_hello())] {
            let mut l = pair(c, h);
            assert_eq!(l.console.agreed().unwrap().caps, caps::HOST_V1 | bit);
            for q in new_requests() {
                let need = caps::of_request(q.family(), q.op());
                let r = l.console.request_any("U", &q);
                if need & NEW & !bit == 0 {
                    assert!(r.is_ok(), "{bit:#x}: {q:?}");
                } else {
                    assert!(matches!(r, Err(Error::NotAgreed { .. })), "{bit:#x}: {q:?}");
                }
            }
        }

        // Events: a console with the bit alone is sent its events and none
        // of the other new ones.
        let limits = Limits::default();
        let mut log = EventLog::new(&limits);
        let console = Hello {
            caps: caps::HOST_V1 | bit,
            ..console_hello()
        };
        let mut l = pair(console, host_hello());
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
        l.pump();
        for e in &samples::events() {
            log.push_any(e).unwrap();
        }
        let sent = l.managed.publish(&log).unwrap();
        l.pump();
        let got: Vec<AnyEvent> = l
            .at_console
            .iter()
            .filter_map(|i| match i {
                Incoming::Event(e) => e.decode().unwrap(),
                _ => None,
            })
            .collect();
        assert_eq!(got.len(), sent);
        let old = samples::events().len() - new_events().len();
        assert_eq!(sent, old + events_of(bit).len(), "{bit:#x}");
        for e in &got {
            let need = caps::of_event(e.family(), e.op());
            assert_eq!(need & NEW & !bit, 0, "{bit:#x}: {e:?}");
        }
    }
}
