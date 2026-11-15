// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hello each way: agreement, the order, the bounds.

mod common;

use common::{closed, replies, request};
use qsi::{Hello, PeerKind, base, caps, family, msgid};
use qsisess::loopback::{Loopback, console_hello, host_hello};
use qsisess::wire::{frame, kind};
use qsisess::{Error, Incoming, Limits, Role, Session};

fn ready(items: &[Incoming]) -> Option<(&Hello, qsi::Agreed)> {
    items.iter().find_map(|i| match i {
        Incoming::Ready { peer, agreed } => Some((peer, *agreed)),
        _ => None,
    })
}

#[test]
fn both_ways() {
    let l = Loopback::new(true);
    let (peer, agreed) = ready(&l.at_console).unwrap();
    assert_eq!(peer, &host_hello());
    assert_eq!(agreed.version, 1);
    assert_eq!(agreed.caps, caps::HOST_V1);
    let (peer, _) = ready(&l.at_managed).unwrap();
    assert_eq!(peer, &console_hello());
    assert!(l.console.is_open() && l.managed.is_open());
    assert_eq!(l.console.peer().unwrap().name, "VS01");
}

#[test]
fn unequal_peers() {
    let console = Hello {
        version: 2,
        caps: caps::MANAGED | caps::PARTITIONS | 1 << 40,
        ..console_hello()
    };
    let mut l = Loopback::with(
        console,
        host_hello(),
        true,
        Limits::default(),
        Limits::default(),
    )
    .unwrap();
    for s in [&l.console, &l.managed] {
        let a = s.agreed().unwrap();
        assert_eq!(a.version, 1);
        assert_eq!(a.caps, caps::MANAGED | caps::PARTITIONS);
    }
    // The console refuses a family the peer lacks before sending it.
    let e = l
        .console
        .request("U", family::VRES, 1, Vec::new())
        .unwrap_err();
    assert_eq!(
        e,
        Error::NotAgreed {
            family: family::VRES
        }
    );
    assert_eq!(e.msgid(), msgid::NOT_AGREED);
    // A console that sends one anyway is refused by the session.
    l.managed
        .feed(&request(9, "U", family::VRES, 1, Vec::new()), 0);
    let r = replies(&l.managed.take_output());
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].refusal().unwrap().0, msgid::NOT_AGREED);
    assert!(l.managed.poll().is_none());
    // An agreed family still works.
    let id = l
        .console
        .request_msg("U", &base::Request::Time(qsi::Empty))
        .unwrap();
    l.pump();
    assert!(l.take_request(id).is_some());
}

#[test]
fn nothing_before_the_hello() {
    let mut s = Session::new(Role::Managed, host_hello(), true, Limits::default(), 0).unwrap();
    assert_eq!(s.reply(1, Vec::new()).unwrap_err(), Error::NotReady);
    s.feed(&frame(kind::PING, &1u64.to_le_bytes()).unwrap(), 0);
    let items: Vec<_> = std::iter::from_fn(|| s.poll()).collect();
    assert_eq!(
        closed(&items),
        Some((Some(msgid::OUT_OF_ORDER.into()), false))
    );
    assert!(s.is_closed());
}

#[test]
fn a_second_hello() {
    let mut l = Loopback::new(true);
    l.managed.feed(
        &frame(kind::HELLO, &ipc::encode(&console_hello())).unwrap(),
        0,
    );
    l.pump();
    assert_eq!(
        closed(&l.at_managed),
        Some((Some(msgid::OUT_OF_ORDER.into()), false))
    );
    assert_eq!(
        closed(&l.at_console),
        Some((Some(msgid::OUT_OF_ORDER.into()), true))
    );
}

#[test]
fn version_zero_and_bad_hellos() {
    let zero = Hello {
        version: 0,
        ..console_hello()
    };
    let mut s = Session::new(Role::Managed, host_hello(), true, Limits::default(), 0).unwrap();
    s.feed(&frame(kind::HELLO, &ipc::encode(&zero)).unwrap(), 0);
    let items: Vec<_> = std::iter::from_fn(|| s.poll()).collect();
    assert_eq!(
        closed(&items).unwrap().0.as_deref(),
        Some(msgid::NO_VERSION)
    );

    // A console talking to a console.
    let mut s = Session::new(Role::Console, console_hello(), true, Limits::default(), 0).unwrap();
    s.feed(
        &frame(kind::HELLO, &ipc::encode(&console_hello())).unwrap(),
        0,
    );
    let items: Vec<_> = std::iter::from_fn(|| s.poll()).collect();
    assert_eq!(
        closed(&items).unwrap().0.as_deref(),
        Some(msgid::WRONG_PEER)
    );

    // A hello that does not read, and one out of bounds.
    let mut s = Session::new(Role::Managed, host_hello(), true, Limits::default(), 0).unwrap();
    s.feed(&frame(kind::HELLO, b"\x01").unwrap(), 0);
    assert!(s.is_closed());
    let long = Hello {
        name: "n".repeat(300),
        ..console_hello()
    };
    let mut s = Session::new(Role::Managed, host_hello(), true, Limits::default(), 0).unwrap();
    s.feed(&frame(kind::HELLO, &ipc::encode(&long)).unwrap(), 0);
    let items: Vec<_> = std::iter::from_fn(|| s.poll()).collect();
    assert_eq!(closed(&items).unwrap().0.as_deref(), Some(msgid::BAD_FRAME));

    // This side's own hello is checked too.
    assert!(matches!(
        Session::new(Role::Console, host_hello(), true, Limits::default(), 0),
        Err(Error::BadHello(_))
    ));
    let mut bad = host_hello();
    bad.kind = PeerKind::Standalone;
    bad.version = 0;
    assert!(Session::new(Role::Managed, bad, true, Limits::default(), 0).is_err());
}

#[test]
fn junk_closes() {
    let mut l = Loopback::new(true);
    l.console.feed(b"junk", 0);
    l.pump();
    assert_eq!(
        closed(&l.at_console),
        Some((Some(msgid::BAD_FRAME.into()), false))
    );
    assert_eq!(
        closed(&l.at_managed),
        Some((Some(msgid::BAD_FRAME.into()), true))
    );
    // A frame of a kind the session does not know.
    let mut l = Loopback::new(true);
    l.managed.feed(&frame(0x10FF, b"").unwrap(), 0);
    assert!(l.managed.is_closed());
}
