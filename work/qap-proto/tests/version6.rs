// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 6: the `Messages` bodies and values, the spool watches and
//! their event, `Stat`, and what each version may be sent.

use std::fmt::Debug;

use ipc::{Wire, decode, encode};
use qap_proto::ops::connection::choose_version;
use qap_proto::ops::event;
use qap_proto::ops::ifs::{Entry, EntryKind, Stat};
use qap_proto::ops::messages::{
    ListMessages, MessageArrived, MessageType, Messages, Queue, QueueRef, QueuedMessage, Recipient,
    RemoveMessages, ReplyMessage, SendMessage, Sent,
};
use qap_proto::ops::spool::{
    SpoolChange, SpoolStatus, SpoolTarget, SpooledFile, SpooledFileChanged, SpooledFileId,
    WatchSpool,
};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Op, Service, Status, VERSION};

fn same<T: Wire + PartialEq + Debug>(v: &T) {
    assert_eq!(&decode::<T>(&encode(v)).unwrap(), v);
}

fn job() -> JobId {
    JobId {
        name: "QPADEV0001".into(),
        user: "ANN".into(),
        number: "012345".into(),
    }
}

fn message(kind: MessageType) -> QueuedMessage {
    QueuedMessage {
        key: [0, 0, 1, 7],
        id: if kind == MessageType::Informational {
            String::new()
        } else {
            "CPA2401".into()
        },
        kind,
        severity: 99,
        text: "Prêt à répondre ?".into(),
        help: "Second level.".into(),
        sent: 1_793_404_800_123,
        from_user: "ANN".into(),
        from_job: job(),
        needs_reply: kind == MessageType::Inquiry,
        reply: (kind == MessageType::SenderCopy).then(|| "G".into()),
        answers: (kind == MessageType::Reply).then_some([0, 0, 1, 6]),
    }
}

fn file() -> SpooledFile {
    SpooledFile {
        id: id(),
        status: SpoolStatus::Held,
        outq: queue("QPRINT"),
        user: "BOB".into(),
        ..SpooledFile::default()
    }
}

fn id() -> SpooledFileId {
    SpooledFileId {
        job: job(),
        file: "QSYSPRT".into(),
        number: 3,
    }
}

fn queue(name: &str) -> ObjectName {
    ObjectName {
        library: "QGPL".into(),
        name: name.into(),
    }
}

#[test]
fn messages_bodies() {
    for (i, k) in MessageType::ALL.iter().enumerate() {
        same(k);
        assert_eq!(encode(k), [u8::try_from(i).unwrap()]);
        same(&message(*k));
    }
    assert!(decode::<MessageType>(&[8]).is_err());
    let queues = [
        Queue::Own,
        Queue::SystemOperator,
        Queue::Named { queue: queue("Q1") },
    ];
    for (i, q) in queues.iter().enumerate() {
        same(q);
        assert_eq!(encode(q)[0], u8::try_from(i).unwrap());
    }
    assert!(decode::<Queue>(&[3]).is_err());
    same(&Recipient::User {
        profile: "ANN".into(),
    });
    same(&Recipient::Queue {
        queue: Queue::SystemOperator,
    });
    assert_eq!(
        encode(&Recipient::User {
            profile: String::new()
        })[0],
        0
    );
    same(&ListMessages {
        queue: Queue::Own,
        newest_first: true,
        start: 50,
        max: 25,
    });
    same(&Messages {
        total: 120,
        messages: MessageType::ALL.iter().map(|k| message(*k)).collect(),
    });
    same(&ReplyMessage {
        queue: Queue::SystemOperator,
        key: [1, 2, 3, 4],
        reply: "G".into(),
    });
    same(&RemoveMessages {
        queue: queues[2].clone(),
        keys: vec![[0; 4], [255; 4]],
    });
    same(&SendMessage {
        to: Recipient::User {
            profile: "BOB".into(),
        },
        text: "Lunch?".into(),
        inquiry: true,
    });
    same(&Sent {
        key: Some([0, 0, 0, 9]),
    });
    same(&Sent { key: None });
    same(&QueueRef { queue: Queue::Own });
    same(&MessageArrived {
        queue: Queue::SystemOperator,
        message: message(MessageType::Inquiry),
    });
}

#[test]
fn layouts() {
    // A key is 4 bytes as they are; a flag one byte.
    let b = encode(&ReplyMessage {
        queue: Queue::Own,
        key: [9, 8, 7, 6],
        reply: String::new(),
    });
    assert_eq!(b, [0, 9, 8, 7, 6, 0, 0, 0, 0]);
    // The reply's `answers` comes last: a tag, then the key.
    let b = encode(&message(MessageType::Reply));
    assert_eq!(b[b.len() - 6..], [0, 1, 0, 0, 1, 6]);
    let mut b = encode(&SendMessage::default());
    *b.last_mut().unwrap() = 2;
    assert!(decode::<SendMessage>(&b).is_err());
}

#[test]
fn spool_watch_bodies() {
    for (i, c) in SpoolChange::ALL.iter().enumerate() {
        same(c);
        assert_eq!(encode(c), [u8::try_from(i).unwrap()]);
    }
    assert!(decode::<SpoolChange>(&[3]).is_err());
    let targets = [
        SpoolTarget::Queue {
            outq: queue("PRT01"),
        },
        SpoolTarget::User {
            profile: "*CURRENT".into(),
        },
    ];
    for (i, t) in targets.iter().enumerate() {
        same(t);
        same(&WatchSpool { target: t.clone() });
        assert_eq!(encode(t)[0], u8::try_from(i).unwrap());
    }
    assert!(decode::<SpoolTarget>(&[2]).is_err());
    same(&SpooledFileChanged {
        target: targets[0].clone(),
        change: SpoolChange::Changed,
        file: Some(file()),
        id: id(),
    });
    same(&SpooledFileChanged {
        target: targets[1].clone(),
        change: SpoolChange::Removed,
        file: None,
        id: id(),
    });
}

#[test]
fn stat_body() {
    same(&Stat {
        entry: Entry {
            name: "a.txt".into(),
            kind: EntryKind::File,
            size: 3,
            modified: 1_793_404_800_123,
            owner: "BOB".into(),
            ccsid: 1208,
            target: None,
        },
        change: u64::MAX - 1,
    });
    // The counter follows the entry, eight bytes.
    let b = encode(&Stat {
        entry: Entry::default(),
        change: 2,
    });
    assert_eq!(b[b.len() - 8..], 2u64.to_le_bytes());
}

#[test]
fn version_6_values() {
    const { assert!(VERSION >= 6) };
    assert_eq!(encode(&Service::Messages), [7]);
    assert_eq!(Service::Messages.since(), 6);
    let names = [
        Op::ListMessages,
        Op::ReplyMessage,
        Op::RemoveMessages,
        Op::SendMessage,
        Op::WatchQueue,
        Op::UnwatchQueue,
    ];
    for (code, want) in (1..=6).zip(names) {
        assert_eq!(Op::of(Service::Messages, code), Some(want));
    }
    assert_eq!(Op::of(Service::Messages, 7), None);
    assert_eq!(Op::of(Service::Spool, 8), Some(Op::WatchSpool));
    assert_eq!(Op::of(Service::Spool, 9), Some(Op::UnwatchSpool));
    assert_eq!(Op::of(Service::Ifs, 9), Some(Op::Stat));
    for op in names
        .into_iter()
        .chain([Op::WatchSpool, Op::UnwatchSpool, Op::Stat])
    {
        assert_eq!(op.since(), 6, "{op:?}");
        assert!(!op.before_signon(), "{op:?}");
        // Concurrent, never in the lane.
        assert!(!op.in_lane(), "{op:?}");
    }
    for v in 1..=5 {
        assert!(!Service::of_version(v).contains(&Service::Messages));
    }
    assert_eq!(Service::of_version(6), Service::ALL[..8].to_vec());
    // Version 6 adds no status.
    assert_eq!(Status::ALL.len(), 16);
    assert!(Status::ALL.iter().all(|s| s.since() <= 5));
}

#[test]
fn events_by_version() {
    assert_eq!(
        event::since(Service::Connection, event::SERVER_ENDING),
        Some(1)
    );
    assert_eq!(event::since(Service::SignOn, event::SESSION_ENDED), Some(1));
    assert_eq!(
        event::since(Service::Messages, event::MESSAGE_ARRIVED),
        Some(6)
    );
    assert_eq!(
        event::since(Service::Spool, event::SPOOLED_FILE_CHANGED),
        Some(6)
    );
    assert_eq!(event::since(Service::Ifs, 100), None);
    let arrived = Envelope::event(Service::Messages, event::MESSAGE_ARRIVED, Status::Ok, None)
        .with_body(&MessageArrived {
            queue: Queue::Own,
            message: message(MessageType::Informational),
        });
    let changed = Envelope::event(
        Service::Spool,
        event::SPOOLED_FILE_CHANGED,
        Status::Ok,
        None,
    );
    let ending = Envelope::event(
        Service::Connection,
        event::SERVER_ENDING,
        Status::ServerEnding,
        None,
    );
    for v in 1..=5 {
        assert!(!arrived.defined_in(v));
        // Spool is version 3's, its event version 6's.
        assert!(!changed.defined_in(v));
        assert!(ending.defined_in(v));
    }
    assert!(arrived.defined_in(6) && changed.defined_in(6) && ending.defined_in(6));
    let body: MessageArrived = arrived.body_as().unwrap();
    assert_eq!(body.queue, Queue::Own);
    // A reply carries its own service's version.
    let req = Envelope::request(4, Op::ListMessages, &ListMessages::default());
    let reply = Envelope::reply(&req, Status::Ok, None, Vec::new());
    assert!(!reply.defined_in(5) && reply.defined_in(6));
    let r = Envelope::reply(&req, Status::Exists, None, Vec::new());
    assert!(
        r.defined_in(6)
            && !Envelope {
                service: Service::Ifs,
                ..r
            }
            .defined_in(4)
    );
}

#[test]
fn negotiation_of_1_to_6() {
    for max in 1..=6 {
        assert_eq!(choose_version(1, max, VERSION), Some(max));
        assert_eq!(choose_version(max, max, VERSION), Some(max));
    }
    assert_eq!(choose_version(1, 9, 6), Some(6));
    assert_eq!(choose_version(6, 6, 6), Some(6));
    assert_eq!(choose_version(7, 9, 6), None);
}
