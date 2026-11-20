// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use std::sync::{Arc, Mutex};

use qsi::any::{AnyReply, AnyRequest};
use qsi::base;
use qsi::host::console;
use qsi::{Empty, pair, service};
use qsisess::{EventLog, Incoming, Limits, Reply, Role, Session};
use qsitrust::{Binding, Fingerprint, Peer};

use super::{Config, Conn};
use crate::fuzzing::{console_hello, standalone_hello};
use crate::host::Host;
use crate::mem::{MemHost, MemKeeper, MemOpener};
use crate::trust::Failure;

const PASSWORD: &str = "7K3Q-9VNM-2HXD-PW4T";

struct Rig {
    conn: Conn,
    console: Session,
    log: Arc<Mutex<EventLog>>,
    keeper: Arc<MemKeeper>,
    now: u64,
    seen: Vec<Incoming>,
}

fn rig(pinned: bool) -> Rig {
    let limits = Limits::default();
    let log = Arc::new(Mutex::new(EventLog::new(&limits)));
    let keeper = Arc::new(MemKeeper::with_password(PASSWORD));
    let config = Config {
        hello: standalone_hello(),
        address: "192.0.2.1:4000".into(),
        peer: Peer {
            cert: Vec::new(),
            fingerprint: Fingerprint([9; 32]),
            pinned,
        },
        binding: Some(Binding {
            mine: Fingerprint([1; 32]),
            peer: Fingerprint([9; 32]),
            exporter: [5; 32],
        }),
        limits,
    };
    let conn = Conn::new(
        config,
        Box::new(MemHost::new()),
        Box::new(MemOpener),
        log.clone(),
        keeper.clone(),
        0,
    )
    .unwrap();
    let console = Session::new(Role::Console, console_hello(), true, limits, 0).unwrap();
    let mut r = Rig {
        conn,
        console,
        log,
        keeper,
        now: 0,
        seen: Vec::new(),
    };
    r.pump();
    r
}

impl Rig {
    fn pump(&mut self) {
        for _ in 0..6 {
            let out = self.console.take_output();
            self.conn.feed(&out, self.now);
            self.conn.step(self.now);
            let back = self.conn.take_output();
            self.console.feed(&back, self.now);
            while let Some(i) = self.console.poll() {
                self.seen.push(i);
            }
        }
    }

    fn ask(&mut self, req: &AnyRequest) -> Reply {
        let id = self.console.request_any("OPER", req).unwrap();
        self.pump();
        let at = self
            .seen
            .iter()
            .position(|i| matches!(i, Incoming::Reply(r) if r.id == id))
            .expect("a reply");
        match self.seen.remove(at) {
            Incoming::Reply(r) => r,
            _ => unreachable!(),
        }
    }

    fn pair(&mut self, password: &str) -> Option<Reply> {
        let binding = Binding {
            mine: Fingerprint([9; 32]),
            peer: Fingerprint([1; 32]),
            exporter: [5; 32],
        };
        let (c, start) = qsitrust::pair::Console::start(password, binding, "MC01").unwrap();
        let r = self.ask(&AnyRequest::Pair(pair::Request::Start(pair::Start {
            spake: start.spake,
            name: start.name,
        })));
        let Ok(Some(pair::Reply::Answer(a))) = r.typed::<pair::Reply>() else {
            return Some(r);
        };
        let answer = qsitrust::pair::Answer {
            spake: a.spake,
            confirm: a.confirm,
            name: a.name,
        };
        let (confirm, paired) = c.answer(&answer).ok()?;
        assert_eq!(paired.fingerprint, Fingerprint([1; 32]));
        Some(
            self.ask(&AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
                confirm: confirm.confirm,
            }))),
        )
    }
}

#[test]
fn a_pinned_console_is_answered_by_every_family() {
    let mut r = rig(true);
    let id = r.ask(&AnyRequest::Base(base::Request::Identify(Empty)));
    match id.decode().unwrap() {
        Some(AnyReply::Base(base::Reply::Identify(i))) => assert_eq!(i.name, "QS400"),
        other => panic!("{other:?}"),
    }
    let s = r.ask(&AnyRequest::Base(base::Request::State(Empty)));
    assert!(s.refusal().is_none());
    let codes = r.ask(&AnyRequest::Base(base::Request::Refcodes(base::Refcodes {
        before: None,
        limit: 2,
    })));
    match codes.decode().unwrap() {
        Some(AnyReply::Base(base::Reply::Refcodes(l))) => {
            assert_eq!(l.codes.len(), 2);
            assert_eq!(l.more, Some(2));
        }
        other => panic!("{other:?}"),
    }
    let take = r.ask(&AnyRequest::Base(base::Request::TakeDump(base::TakeDump {
        kind: base::DumpKind::ServiceData,
        kind_name: String::new(),
    })));
    assert!(take.refusal().is_none());
    let other = r.ask(&AnyRequest::Base(base::Request::TakeDump(base::TakeDump {
        kind: base::DumpKind::System,
        kind_name: "x".into(),
    })));
    assert_eq!(other.refusal().unwrap().0, "QSI1203");
    let note = r.ask(&AnyRequest::Service(service::Request::Note(
        service::Note {
            number: 2_632_400_001,
            text: "looked at it".into(),
        },
    )));
    assert!(note.refusal().is_none());
    let close = r.ask(&AnyRequest::Service(service::Request::Close(
        service::Close {
            number: 2_632_400_001,
            anyway: false,
        },
    )));
    assert!(close.refusal().is_none());
    let again = r.ask(&AnyRequest::Service(service::Request::Close(
        service::Close {
            number: 2_632_400_001,
            anyway: false,
        },
    )));
    assert_eq!(again.refusal().unwrap().0, "QSI1003");
    let p1 = r.ask(&AnyRequest::Console(console::Request::Attach(
        console::Attach {
            id: 1,
            take_over: false,
        },
    )));
    assert_eq!(p1.refusal().unwrap().0, "QSI0331");
    let c0 = r.ask(&AnyRequest::Console(console::Request::Attach(
        console::Attach {
            id: 0,
            take_over: false,
        },
    )));
    assert_eq!(c0.refusal().unwrap().0, "QSI0301");
    // A dump downloaded on a stream.
    let dl = r.ask(&AnyRequest::Base(base::Request::DownloadDump(
        base::DownloadDump {
            name: "QSD0000001".into(),
        },
    )));
    let stream = dl.stream.expect("a stream");
    r.pump();
    let got: usize = r
        .seen
        .iter()
        .filter_map(|i| match i {
            Incoming::Data { stream: s, bytes } if *s == stream => Some(bytes.len()),
            _ => None,
        })
        .sum();
    assert_eq!(got, 1000);
    // Pairing again is refused.
    let p = r.ask(&AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
        confirm: vec![0; 32],
    })));
    assert_eq!(p.refusal().unwrap().0, "QSI0203");
}

#[test]
fn events_are_sent_from_the_shared_log() {
    let mut r = rig(true);
    crate::conn::lock(&r.log)
        .push_msg(&base::Event::State(MemHost::new().state().unwrap()))
        .unwrap();
    let sub = r.ask(&AnyRequest::Base(base::Request::Subscribe(
        base::Subscribe { since: Some(0) },
    )));
    assert!(sub.refusal().is_none());
    r.pump();
    assert!(
        r.seen
            .iter()
            .any(|i| matches!(i, Incoming::Event(e) if e.seq == 1))
    );
}

#[test]
fn an_unpinned_console_pairs_and_nothing_else() {
    let mut r = rig(false);
    let id = r.ask(&AnyRequest::Base(base::Request::Identify(Empty)));
    assert_eq!(id.refusal().unwrap().0, "QSI0101");
    let early = r.ask(&AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
        confirm: vec![0; 32],
    })));
    assert_eq!(early.refusal().unwrap().0, "QSI0204");
    // A wrong password: the console finds the answer wrong and stops.
    assert!(r.pair("0000-0000-0000-0000").is_none());
    r.conn.gone();
    assert_eq!(
        r.keeper.failures().last().map(|f| f.1),
        Some(Failure::Abandoned)
    );
    let done = r.pair(PASSWORD).expect("confirmed");
    assert!(done.refusal().is_none(), "{:?}", done.refusal());
    assert_eq!(r.keeper.pins().len(), 1);
    assert!(!r.keeper.out());
    assert_eq!(r.conn.paired(), Some(Fingerprint([9; 32])));
}

#[test]
fn five_attempts_and_the_password_is_spent() {
    let mut r = rig(false);
    for _ in 0..5 {
        assert!(r.pair("0000-0000-0000-0000").is_none());
    }
    let spent = r.pair(PASSWORD).expect("refused");
    assert_eq!(spent.refusal().unwrap().0, "QSI0201");
    assert!(
        r.keeper
            .failures()
            .iter()
            .any(|f| f.1 == Failure::NoPassword)
    );
}
