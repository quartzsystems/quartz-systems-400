// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 6's `Messages` service against the test system's queues:
//! every operation and status, keys and order, inquiries matched with
//! their replies, watches beside a queue held in break mode, their
//! events in arrival order, and their end with the connection and a
//! sign-off.

#![allow(clippy::many_single_char_names)] // keys, runs and queues

mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::event;
use qap_proto::ops::messages::{
    Key, ListMessages, MessageArrived, MessageType, Messages, Queue, QueueRef, QueuedMessage,
    Recipient, RemoveMessages, ReplyMessage, SendMessage, Sent,
};
use qap_proto::ops::signon::{Resume, SignOn, SignOnReply};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Envelope, Kind, Op, Secret, Service, Status};
use qapd::Config;
use qapd::testing::messages::{BREAK_JOB, Msg, OPERATOR_FILLER};

use common::{Controls, Run, cfg, start_with};

/// Watches look often in tests.
fn quick() -> Config {
    Config {
        watch_poll: Duration::from_millis(20),
        ..cfg()
    }
}

/// Version 6, signed on as `user`; the session's reply.
fn signed_on(c: &Arc<Controls>, user: &str, password: &str) -> (Run, SignOnReply) {
    let mut r = start_with(quick(), Arc::clone(c));
    let h: HelloReply = r.client.hello_range(1, 6).body_as().unwrap();
    assert_eq!(h.version, 6);
    assert!(h.services.contains(&Service::Messages));
    let s = r.client.call(
        Op::SignOn,
        &SignOn {
            user: user.into(),
            password: Secret::from(password),
        },
    );
    assert_eq!(s.status, Status::Ok);
    let reply = s.body_as().unwrap();
    (r, reply)
}

fn bob(c: &Arc<Controls>) -> Run {
    signed_on(c, "BOB", "SECRET1").0
}

fn named(library: &str, name: &str) -> Queue {
    Queue::Named {
        queue: ObjectName {
            library: library.into(),
            name: name.into(),
        },
    }
}

fn list(r: &mut Run, queue: Queue, newest_first: bool, start: u32, max: u32) -> Envelope {
    r.client.call(
        Op::ListMessages,
        &ListMessages {
            queue,
            newest_first,
            start,
            max,
        },
    )
}

fn messages(r: &mut Run, queue: Queue) -> Vec<QueuedMessage> {
    let e = list(r, queue, false, 0, 0);
    assert_eq!(e.status, Status::Ok, "{:?}", e.message);
    e.body_as::<Messages>().unwrap().messages
}

fn refused(e: &Envelope, status: Status, id: &str) {
    assert_eq!(e.status, status, "{e:?}");
    assert_eq!(e.message.as_ref().map(|m| m.id.as_str()), Some(id), "{e:?}");
    assert!(e.body.is_empty());
}

fn reply(r: &mut Run, queue: Queue, key: Key, text: &str) -> Envelope {
    r.client.call(
        Op::ReplyMessage,
        &ReplyMessage {
            queue,
            key,
            reply: text.into(),
        },
    )
}

fn send(r: &mut Run, to: Recipient, text: &str, inquiry: bool) -> Envelope {
    r.client.call(
        Op::SendMessage,
        &SendMessage {
            to,
            text: text.into(),
            inquiry,
        },
    )
}

fn watch(r: &mut Run, queue: Queue) -> Envelope {
    r.client.call(Op::WatchQueue, &QueueRef { queue })
}

fn impromptu(text: &str) -> Msg {
    Msg {
        kind: MessageType::Informational,
        text: text.into(),
        from: JobId {
            name: "QPADEV0009".into(),
            user: "ANN".into(),
            number: "001300".into(),
        },
        ..Msg::default()
    }
}

/// The next frame, which must be a `MessageArrived` event.
fn arrived(r: &mut Run) -> MessageArrived {
    let e = r.client.recv().expect("an event");
    assert_eq!(
        (e.kind, e.service, e.op, e.status, e.request_id),
        (
            Kind::Event,
            Service::Messages,
            event::MESSAGE_ARRIVED,
            Status::Ok,
            0
        ),
        "{e:?}"
    );
    e.body_as().unwrap()
}

/// Nothing comes for a while: several looks of the watches.
fn quiet(r: &mut Run) {
    if let Some(e) = r.client.try_recv(Duration::from_millis(300)) {
        panic!("nothing should come: {e:?}");
    }
}

#[test]
fn lists_every_type_in_arrival_order() {
    let c = Controls::new();
    let mut r = bob(&c);
    let own = messages(&mut r, Queue::Own);
    let kinds: Vec<MessageType> = own.iter().map(|m| m.kind).collect();
    for k in MessageType::ALL {
        assert!(kinds.contains(&k), "{k:?} on BOB's queue");
    }
    // An identifier's text is in the job's language; an impromptu
    // message's as sent.
    let done = own
        .iter()
        .find(|m| m.kind == MessageType::Completion)
        .unwrap();
    assert_eq!(done.id, "CPC2102");
    assert_eq!(done.text, "CPC2102 HLTEST (lang of BOB)");
    assert_eq!(done.help, "Help for CPC2102.");
    let info = own
        .iter()
        .find(|m| m.kind == MessageType::Informational)
        .unwrap();
    assert!(info.id.is_empty() && info.text.starts_with("Backups"));
    // The inquiry waits; the copy has its reply, and the reply answers it.
    let inq = own.iter().find(|m| m.kind == MessageType::Inquiry).unwrap();
    assert!(inq.needs_reply && inq.reply.is_none() && inq.answers.is_none());
    let copy = own
        .iter()
        .find(|m| m.kind == MessageType::SenderCopy)
        .unwrap();
    assert!(!copy.needs_reply);
    assert_eq!(copy.reply.as_deref(), Some("OK"));
    let rpy = own.iter().find(|m| m.kind == MessageType::Reply).unwrap();
    assert_eq!(rpy.answers, Some(copy.key));
    assert_eq!(rpy.from_user, "ANN");
    // QSYSOPR: more than a page, strictly newest first when asked.
    let all = list(&mut r, Queue::SystemOperator, false, 0, 0);
    let all: Messages = all.body_as().unwrap();
    assert_eq!(all.total as usize, 9 + OPERATOR_FILLER);
    assert_eq!(all.messages.len(), all.total as usize);
    let keys: HashSet<Key> = all.messages.iter().map(|m| m.key).collect();
    assert_eq!(keys.len(), all.messages.len(), "keys are unique");
    assert!(all.messages.windows(2).all(|w| w[0].sent <= w[1].sent));
    let mut newest = Vec::new();
    let mut start = 0;
    loop {
        let p: Messages = list(&mut r, Queue::SystemOperator, true, start, 50)
            .body_as()
            .unwrap();
        assert_eq!(p.total, all.total);
        if p.messages.is_empty() {
            break;
        }
        assert!(p.messages.len() <= 50);
        start += 50;
        newest.extend(p.messages);
    }
    let mut back = all.messages.clone();
    back.reverse();
    assert_eq!(newest, back);
    // Paging past the end is empty, not an error.
    let past: Messages = list(&mut r, Queue::SystemOperator, false, 5000, 10)
        .body_as()
        .unwrap();
    assert!(past.messages.is_empty());
    // An operator inquiry with valid replies and a default.
    let cpa = all.messages.iter().find(|m| m.id == "CPA0701").unwrap();
    assert!(cpa.needs_reply && cpa.kind == MessageType::Inquiry && cpa.severity == 99);
}

#[test]
fn not_found_and_not_authorized() {
    let c = Controls::new();
    let mut r = bob(&c);
    let nope = named("QGPL", "NOPE");
    let secret = named("QGPL", "SECRETQ");
    let anns = named("QUSRSYS", "ANN");
    refused(
        &list(&mut r, nope.clone(), false, 0, 0),
        Status::NotFound,
        "CPF2403",
    );
    refused(
        &list(&mut r, secret.clone(), false, 0, 0),
        Status::NotAuthorized,
        "CPF9802",
    );
    refused(
        &list(&mut r, anns.clone(), false, 0, 0),
        Status::NotAuthorized,
        "CPF9802",
    );
    for q in [nope.clone(), secret.clone()] {
        let want = if q == nope {
            (Status::NotFound, "CPF2403")
        } else {
            (Status::NotAuthorized, "CPF9802")
        };
        refused(&reply(&mut r, q.clone(), [0, 0, 0, 1], "Y"), want.0, want.1);
        let rm = r.client.call(
            Op::RemoveMessages,
            &RemoveMessages {
                queue: q.clone(),
                keys: vec![[0, 0, 0, 1]],
            },
        );
        refused(&rm, want.0, want.1);
        refused(
            &send(&mut r, Recipient::Queue { queue: q.clone() }, "hi", false),
            want.0,
            want.1,
        );
        refused(&watch(&mut r, q.clone()), want.0, want.1);
    }
    refused(
        &send(
            &mut r,
            Recipient::User {
                profile: "NOBODY".into(),
            },
            "hi",
            false,
        ),
        Status::NotFound,
        "CPF2204",
    );
    // Through `*LIBL` too.
    let libl = named("*LIBL", "TEAMQ");
    assert_eq!(list(&mut r, libl, false, 0, 1).status, Status::Ok);
}

#[test]
fn replies_are_checked_as_the_reply_command_checks_them() {
    let c = Controls::new();
    let mut r = bob(&c);
    let op = messages(&mut r, Queue::SystemOperator);
    let cpa = op.iter().find(|m| m.id == "CPA0701").unwrap().key;
    let info = op
        .iter()
        .find(|m| m.kind == MessageType::Informational)
        .unwrap()
        .key;
    let done = op
        .iter()
        .find(|m| m.kind == MessageType::Inquiry && m.reply.is_some())
        .unwrap()
        .key;
    refused(
        &reply(&mut r, Queue::SystemOperator, [9, 9, 9, 9], "C"),
        Status::NotFound,
        "CPF2410",
    );
    refused(
        &reply(&mut r, Queue::SystemOperator, info, "C"),
        Status::InvalidRequest,
        "CPF2422",
    );
    refused(
        &reply(&mut r, Queue::SystemOperator, done, "C"),
        Status::InvalidRequest,
        "CPF2422",
    );
    // Not one of C, D, I, R: the system's message.
    let bad = reply(&mut r, Queue::SystemOperator, cpa, "X");
    refused(&bad, Status::Error, "CPF2420");
    assert!(bad.message.unwrap().text.contains("X CPA0701"));
    // Any case of a valid one, and it is answered once.
    assert_eq!(
        reply(&mut r, Queue::SystemOperator, cpa, "r").status,
        Status::Ok
    );
    refused(
        &reply(&mut r, Queue::SystemOperator, cpa, "R"),
        Status::InvalidRequest,
        "CPF2422",
    );
    let after = messages(&mut r, Queue::SystemOperator);
    let m = after.iter().find(|m| m.key == cpa).unwrap();
    assert!(!m.needs_reply);
    assert_eq!(m.reply.as_deref(), Some("r"));
    let last = after.last().unwrap();
    assert_eq!(
        (last.kind, last.answers, last.from_user.as_str()),
        (MessageType::Reply, Some(cpa), "BOB")
    );
    // An empty reply is the default one.
    let printer = after
        .iter()
        .find(|m| m.text.starts_with("Is the forms printer"))
        .unwrap()
        .key;
    assert_eq!(
        reply(&mut r, Queue::SystemOperator, printer, "  ").status,
        Status::Ok
    );
    let m = messages(&mut r, Queue::SystemOperator)
        .into_iter()
        .find(|m| m.key == printer)
        .unwrap();
    assert_eq!(m.reply.as_deref(), Some("NO"));
}

#[test]
fn remove_is_all_or_none_and_keys_are_not_reused() {
    let c = Controls::new();
    let mut r = bob(&c);
    let own = messages(&mut r, Queue::Own);
    let (a, b) = (own[0].key, own[1].key);
    let rm = |r: &mut Run, keys: Vec<Key>| {
        r.client.call(
            Op::RemoveMessages,
            &RemoveMessages {
                queue: Queue::Own,
                keys,
            },
        )
    };
    refused(
        &rm(&mut r, vec![a, [7, 7, 7, 7]]),
        Status::NotFound,
        "CPF2410",
    );
    assert_eq!(
        messages(&mut r, Queue::Own).len(),
        own.len(),
        "none removed"
    );
    assert_eq!(rm(&mut r, vec![a, b]).status, Status::Ok);
    let left = messages(&mut r, Queue::Own);
    assert_eq!(left.len(), own.len() - 2);
    assert!(left.iter().all(|m| m.key != a && m.key != b));
    // The newest goes; the next message gets a key no message had.
    let newest = left.last().unwrap().key;
    assert_eq!(rm(&mut r, vec![newest]).status, Status::Ok);
    assert_eq!(
        send(
            &mut r,
            Recipient::Queue { queue: Queue::Own },
            "note to self",
            false
        )
        .status,
        Status::Ok
    );
    let now = messages(&mut r, Queue::Own);
    let added = now.last().unwrap();
    assert_eq!(added.text, "note to self");
    assert!(own.iter().all(|m| m.key != added.key));
    let sent: Sent = send(
        &mut r,
        Recipient::Queue { queue: Queue::Own },
        "again",
        false,
    )
    .body_as()
    .unwrap();
    assert_eq!(sent.key, None, "only an inquiry has a key");
    // No text.
    let e = send(&mut r, Recipient::Queue { queue: Queue::Own }, " ", false);
    assert_eq!(e.status, Status::InvalidRequest);
}

#[test]
fn an_inquiry_finds_its_reply() {
    let c = Controls::new();
    let mut b = bob(&c);
    let (mut a, _) = signed_on(&c, "ANN", "ANNPW01");
    assert_eq!(watch(&mut b, Queue::Own).status, Status::Ok);
    let s = send(
        &mut b,
        Recipient::User {
            profile: "ANN".into(),
        },
        "Lunch at one?",
        true,
    );
    assert_eq!(s.status, Status::Ok);
    let key = s.body_as::<Sent>().unwrap().key.expect("an inquiry's key");
    // BOB's own queue has the sender's copy under that key.
    let copy = arrived(&mut b);
    assert_eq!(copy.queue, Queue::Own);
    assert_eq!(
        (copy.message.kind, copy.message.key),
        (MessageType::SenderCopy, key)
    );
    assert!(!copy.message.needs_reply && copy.message.reply.is_none());
    // ANN has the inquiry, under its own key.
    let hers = messages(&mut a, Queue::Own);
    let inq = hers.iter().find(|m| m.text == "Lunch at one?").unwrap();
    assert!(inq.needs_reply && inq.kind == MessageType::Inquiry);
    assert_eq!(inq.from_user, "BOB");
    assert_eq!(reply(&mut a, Queue::Own, inq.key, "YES").status, Status::Ok);
    // BOB's reply answers the key SendMessage gave.
    let rpy = arrived(&mut b);
    assert_eq!(
        (rpy.message.kind, rpy.message.answers),
        (MessageType::Reply, Some(key))
    );
    assert_eq!(rpy.message.text, "YES");
    assert_eq!(rpy.message.from_user, "ANN");
    let own = messages(&mut b, Queue::Own);
    let copy = own.iter().find(|m| m.key == key).unwrap();
    assert_eq!(copy.reply.as_deref(), Some("YES"));
    // ANN's queue keeps the inquiry answered and a reply of its own.
    let hers = messages(&mut a, Queue::Own);
    let inq2 = hers.iter().find(|m| m.key == inq.key).unwrap();
    assert!(!inq2.needs_reply && inq2.reply.as_deref() == Some("YES"));
    assert!(
        hers.iter()
            .any(|m| m.kind == MessageType::Reply && m.answers == Some(inq.key))
    );
    // To one's own queue: the inquiry's key, answered there.
    let s = send(
        &mut b,
        Recipient::Queue { queue: Queue::Own },
        "Remember the backup?",
        true,
    );
    let key = s.body_as::<Sent>().unwrap().key.unwrap();
    let inq = arrived(&mut b);
    assert_eq!(
        (inq.message.kind, inq.message.key),
        (MessageType::Inquiry, key)
    );
    assert_eq!(reply(&mut b, Queue::Own, key, "DONE").status, Status::Ok);
    let rpy = arrived(&mut b);
    assert_eq!(rpy.message.answers, Some(key));
    quiet(&mut b);
}

#[test]
fn events_come_in_arrival_order_per_queue() {
    let c = Controls::new();
    let mut r = bob(&c);
    assert_eq!(watch(&mut r, Queue::Own).status, Status::Ok);
    assert_eq!(watch(&mut r, Queue::SystemOperator).status, Status::Ok);
    // What was there before the watch is not reported.
    quiet(&mut r);
    let mut want_op = Vec::new();
    let mut want_own = Vec::new();
    for i in 0..40 {
        want_op.push(
            c.inject_message("QSYS", "QSYSOPR", impromptu(&format!("op {i}")))
                .unwrap(),
        );
        if i % 3 == 0 {
            want_own.push(
                c.inject_message("QUSRSYS", "BOB", impromptu(&format!("own {i}")))
                    .unwrap(),
            );
        }
    }
    let (mut got_op, mut got_own) = (Vec::new(), Vec::new());
    while got_op.len() + got_own.len() < want_op.len() + want_own.len() {
        let e = arrived(&mut r);
        match e.queue {
            Queue::SystemOperator => got_op.push(e.message.key),
            Queue::Own => got_own.push(e.message.key),
            q @ Queue::Named { .. } => panic!("not watched: {q:?}"),
        }
    }
    assert_eq!(got_op, want_op, "QSYSOPR in arrival order, each once");
    assert_eq!(got_own, want_own);
    quiet(&mut r);
    // An unwatch stops them; a second one finds nothing to stop.
    assert_eq!(
        r.client
            .call(Op::UnwatchQueue, &QueueRef { queue: Queue::Own })
            .status,
        Status::Ok
    );
    refused(
        &r.client
            .call(Op::UnwatchQueue, &QueueRef { queue: Queue::Own }),
        Status::NotFound,
        "TCP1A5B",
    );
    c.inject_message("QUSRSYS", "BOB", impromptu("unwatched"))
        .unwrap();
    let k = c
        .inject_message("QSYS", "QSYSOPR", impromptu("still watched"))
        .unwrap();
    assert_eq!(arrived(&mut r).message.key, k);
    quiet(&mut r);
}

#[test]
fn watches_are_passive_beside_break_mode() {
    let c = Controls::new();
    let mut r = bob(&c);
    let team = named("QGPL", "TEAMQ");
    assert_eq!(c.break_holder("QGPL", "TEAMQ"), Some(BREAK_JOB));
    // Not InUse: the watch takes nothing from the job in break mode.
    assert_eq!(watch(&mut r, team.clone()).status, Status::Ok);
    let (mut other, _) = signed_on(&c, "WARN", "WARNPW1");
    assert_eq!(watch(&mut other, team.clone()).status, Status::Ok);
    assert_eq!(c.break_holder("QGPL", "TEAMQ"), Some(BREAK_JOB));
    let k = c
        .inject_message("QGPL", "TEAMQ", impromptu("Stand-up in five minutes."))
        .unwrap();
    for x in [&mut r, &mut other] {
        let e = arrived(x);
        assert_eq!((e.queue.clone(), e.message.key), (team.clone(), k));
    }
    assert_eq!(c.break_holder("QGPL", "TEAMQ"), Some(BREAK_JOB));
    // Messages removed meanwhile do not upset the watch.
    let all = messages(&mut r, team.clone());
    let keys: Vec<Key> = all.iter().map(|m| m.key).collect();
    let rm = r.client.call(
        Op::RemoveMessages,
        &RemoveMessages {
            queue: team.clone(),
            keys,
        },
    );
    assert_eq!(rm.status, Status::Ok);
    let k2 = c
        .inject_message("QGPL", "TEAMQ", impromptu("Moved to the big room."))
        .unwrap();
    assert_eq!(arrived(&mut r).message.key, k2);
    assert_eq!(arrived(&mut other).message.key, k2);
    quiet(&mut r);
}

#[test]
fn a_watch_ends_with_its_connection() {
    let c = Controls::new();
    let (mut r, session) = signed_on(&c, "BOB", "SECRET1");
    assert_eq!(watch(&mut r, Queue::Own).status, Status::Ok);
    drop(r.client);
    r.server.join().unwrap();
    // Resume on a new connection: the session, not its watches.
    let mut n = start_with(quick(), Arc::clone(&c));
    n.client.hello_range(1, 6);
    let back = n.client.call(
        Op::Resume,
        &Resume {
            token: session.token.clone(),
        },
    );
    assert_eq!(back.status, Status::Ok);
    c.inject_message("QUSRSYS", "BOB", impromptu("while away"))
        .unwrap();
    quiet(&mut n);
    refused(
        &n.client
            .call(Op::UnwatchQueue, &QueueRef { queue: Queue::Own }),
        Status::NotFound,
        "TCP1A5B",
    );
    // Watch again and catch up with a list.
    assert_eq!(watch(&mut n, Queue::Own).status, Status::Ok);
    let caught = messages(&mut n, Queue::Own);
    assert_eq!(caught.last().unwrap().text, "while away");
    let k = c
        .inject_message("QUSRSYS", "BOB", impromptu("back"))
        .unwrap();
    assert_eq!(arrived(&mut n).message.key, k);
    // A sign-off ends them too.
    assert_eq!(n.client.call(Op::SignOff, &()).status, Status::Ok);
    n.client.sign_on("BOB", "SECRET1");
    c.inject_message("QUSRSYS", "BOB", impromptu("after sign-off"))
        .unwrap();
    quiet(&mut n);
}

#[test]
fn not_before_sign_on() {
    let c = Controls::new();
    let mut r = start_with(quick(), Arc::clone(&c));
    r.client.hello_range(6, 6);
    let e = r.client.call(Op::ListMessages, &ListMessages::default());
    assert_eq!(e.status, Status::NotSignedOn);
    let e = r.client.call(Op::WatchQueue, &QueueRef::default());
    assert_eq!(e.status, Status::NotSignedOn);
    r.client.sign_on("BOB", "SECRET1");
    let e = Envelope {
        body: vec![9, 9],
        ..Envelope::request(80, Op::WatchQueue, &())
    };
    r.client.send(&e);
    assert_eq!(r.client.reply(80).status, Status::InvalidRequest);
}
