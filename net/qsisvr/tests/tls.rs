// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server over real TLS on loopback: a console pairs, connects again
//! pinned and is answered; before its pin it is answered only for
//! pairing; a wrong password, a replayed exchange and a console nobody
//! pinned are refused; a pin removed ends the session; events reach a
//! subscriber.

use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qsi::any::{AnyReply, AnyRequest};
use qsi::{Empty, base};
use qsisess::{EventLog, Limits};
use qsisvr::client::{Client, ConsolePins, hello};
use qsisvr::fuzzing::standalone_hello;
use qsisvr::mem::{MemHost, MemKeeper, MemOpener};
use qsisvr::serve::{Running, Server, listen};
use qsisvr::trust::SharedPins;
use qsitrust::{Fingerprint, Identity};

const PASSWORD: &str = "7K3Q-9VNM-2HXD-PW4T";
const WAIT: Duration = Duration::from_secs(10);

struct Rig {
    addr: SocketAddr,
    server: Arc<Server>,
    keeper: Arc<MemKeeper>,
    running: Option<Running>,
}

fn rig() -> Rig {
    let id = Identity::make("QS400", "S0000001", 1_794_488_700).unwrap();
    let pins = SharedPins::default();
    pins.set([], true);
    let keeper = Arc::new(MemKeeper::with_password(PASSWORD));
    let log = Arc::new(Mutex::new(EventLog::new(&Limits::default())));
    let server = Arc::new(
        Server::new(
            id,
            pins,
            standalone_hello(),
            log,
            keeper.clone(),
            Box::new(|| {
                Ok((
                    Box::new(MemHost::new()) as Box<dyn qsisvr::Host>,
                    Box::new(MemOpener) as Box<dyn qsisvr::Opener>,
                ))
            }),
        )
        .unwrap(),
    );
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let running = listen(l, server.clone()).unwrap();
    Rig {
        addr,
        server,
        keeper,
        running: Some(running),
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some(r) = self.running.take() {
            r.stop(&self.server);
        }
    }
}

fn identify(c: &mut Client) -> qsisess::Reply {
    c.ask(&AnyRequest::Base(base::Request::Identify(Empty)), WAIT)
        .unwrap()
}

#[test]
fn a_console_pairs_then_is_answered_pinned() {
    let r = rig();
    let me = Identity::make("MC01", "", 1_794_488_700).unwrap();
    let pins = ConsolePins::default();
    pins.pairing(true);
    let mut c = Client::connect(r.addr, &me, &pins, hello("MC01"), WAIT).unwrap();
    assert_eq!(identify(&mut c).refusal().unwrap().0, "QSI0101");
    let (fp, name) = c.pair(PASSWORD, "MC01", WAIT).unwrap();
    assert_eq!(name, "QS400");
    pins.pin(fp);
    c.close();
    assert_eq!(r.keeper.pins()[0].fingerprint, me.fingerprint());
    // Pinned both ways now.
    let mut c = Client::connect(r.addr, &me, &pins, hello("MC01"), WAIT).unwrap();
    match identify(&mut c).decode().unwrap() {
        Some(AnyReply::Base(base::Reply::Identify(i))) => assert_eq!(i.name, "QS400"),
        other => panic!("{other:?}"),
    }
    // An event pushed reaches a subscriber.
    let sub = c
        .ask(
            &AnyRequest::Base(base::Request::Subscribe(base::Subscribe { since: None })),
            WAIT,
        )
        .unwrap();
    assert!(sub.refusal().is_none());
    qsisvr::conn::lock(&r.server.log)
        .push_msg(&base::Event::DumpRemoved(base::DumpRemoved {
            name: "QSD0000001".into(),
            time: 1,
        }))
        .unwrap();
    assert!(!c.events_for(Duration::from_millis(500)).is_empty());
    // The pin removed: the session ends within a second or so.
    r.server.pins.set([], false);
    let end = std::time::Instant::now() + WAIT;
    while !c.is_closed() && std::time::Instant::now() < end {
        c.step();
    }
    assert!(c.is_closed());
    // And a new connection does not get past TLS.
    assert!(Client::connect(r.addr, &me, &pins, hello("MC01"), WAIT).is_err());
}

#[test]
fn wrong_passwords_and_a_replay_fail() {
    let r = rig();
    let me = Identity::make("MC02", "", 1_794_488_700).unwrap();
    let pins = ConsolePins::default();
    pins.pairing(true);
    let mut c = Client::connect(r.addr, &me, &pins, hello("MC02"), WAIT).unwrap();
    let e = c.pair("0000-0000-0000-0000", "MC02", WAIT).unwrap_err();
    assert!(e.contains("wrong password"), "{e}");
    c.close();
    // A replay of a whole good exchange on a new connection fails: the
    // exchange is bound to its connection. Record one against a second
    // system with the same password.
    let other = rig();
    let mut o = Client::connect(other.addr, &me, &pins, hello("MC02"), WAIT).unwrap();
    let (start, confirm, ok) = o.pair_kept(PASSWORD, "MC02", WAIT).unwrap();
    assert!(ok);
    o.close();
    let mut c = Client::connect(r.addr, &me, &pins, hello("MC02"), WAIT).unwrap();
    let (_, done) = c.pair_raw(&start, &confirm, WAIT).unwrap();
    assert_eq!(done.refusal().unwrap().0, "QSI0202");
    c.close();
    std::thread::sleep(Duration::from_millis(200));
    assert!(r.keeper.pins().is_empty());
    assert!(r.keeper.failures().len() >= 2);
    // A console with no password out and not pinned gets no session.
    r.server.pins.set([Fingerprint([1; 32])], false);
    assert!(Client::connect(r.addr, &me, &pins, hello("MC02"), WAIT).is_err());
}
