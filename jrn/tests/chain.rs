// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journals and receivers: creation, appending, the threshold swap,
//! `CHGJRN`, the chain cursor, `DSPJRN` filters, deletion rules.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use jrn::journal::{CrtJrnOpts, Journal, NewReceiver, crtjrn, dltjrn};
use jrn::rcv::{CrtJrnRcvOpts, RcvStatus, Receiver, crtjrnrcv, dltjrnrcv};
use jrn::{
    ChainCursor, Draft, Entry, Filter, JrnError, JrnObjEntry, code, dspjrn, dspjrnrcva, render,
};
use qsname::Name;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn open_new() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let lib = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        let lib = ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: name("QSTEST"),
                type_code: types::LIB,
                attribute: String::new(),
                text: "Test library".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: 1_767_225_600,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        lib
    };
    (store, lib)
}

const NOW: u64 = 1_767_225_600;

#[test]
fn receivers_entries_and_the_chain() {
    let (mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let rcv1 = crtjrnrcv(
        &mut t,
        lib,
        name("RCV0001"),
        &CrtJrnRcvOpts {
            text: "First receiver".into(),
            threshold_kb: 1,
            now: NOW,
            ..CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    let r = Receiver::load(&mut t, rcv1).unwrap();
    assert_eq!(r.attr.status, RcvStatus::Empty);
    // A receiver is no journal.
    assert!(matches!(
        Journal::load(&mut t, rcv1),
        Err(JrnError::NotAJournal { .. })
    ));
    let jid = crtjrn(
        &mut t,
        lib,
        name("QSTJRN"),
        rcv1,
        &CrtJrnOpts {
            text: "Test journal".into(),
            now: NOW,
            ..CrtJrnOpts::default()
        },
    )
    .unwrap();
    // The same receiver cannot start a second journal.
    let e = crtjrn(&mut t, lib, name("OTHER"), rcv1, &CrtJrnOpts::default()).unwrap_err();
    assert!(matches!(e, JrnError::ReceiverUsed { .. }));
    assert_eq!(e.msgid(), Some("CPF7010"));
    let mut j = Journal::load(&mut t, jid).unwrap();
    assert_eq!(j.attr.attached, rcv1);
    assert_eq!(j.attr.next_seq, 1);
    let r = Receiver::load(&mut t, rcv1).unwrap();
    assert_eq!((r.attr.status, r.attr.journal), (RcvStatus::Attached, jid));
    assert_eq!(r.attr.attached, NOW);
    // User entries number from 1 and land in the attached receiver.
    let s1 = j.sndjrne(&mut t, JobHandle(7), "01", b"hello").unwrap();
    let s2 = j
        .append(
            &mut t,
            Draft::new(code::USER, "02", JobHandle(8))
                .data(vec![1; 40])
                .at(NOW + 5),
        )
        .unwrap();
    assert_eq!((s1, s2), (1, 2));
    let r = Receiver::load(&mut t, rcv1).unwrap();
    assert_eq!(
        (r.attr.first_seq, r.attr.last_seq, r.attr.entry_count),
        (1, 2, 2)
    );
    assert_eq!(r.attr.bytes_used, 128 + 5 + 128 + 40);
    let entries = r.entries(&mut t).unwrap();
    assert_eq!(entries[0].type_name(), "U 01");
    assert_eq!(entries[0].data, b"hello");
    assert_eq!(entries[0].job.number, 7);
    assert!(entries[0].time >= NOW);
    assert_eq!((entries[1].time, entries[1].micros), (NOW + 5, 0));
    // Filling past 1 KiB swaps receivers: RCV0002 is generated.
    for i in 0..6u8 {
        j.append(
            &mut t,
            Draft::new(code::USER, "03", JobHandle(7)).data(vec![i; 100]),
        )
        .unwrap();
    }
    let j = Journal::load(&mut t, jid).unwrap();
    assert_ne!(j.attr.attached, rcv1);
    let rcv2 = j.attr.attached;
    let r2 = Receiver::load(&mut t, rcv2).unwrap();
    assert_eq!(r2.name(), name("RCV0002"));
    assert_eq!(r2.attr.threshold_kb, 1);
    assert_eq!(ccsid::decode_trimmed(&r2.header.text), "First receiver");
    assert_eq!(r2.attr.prev, rcv1);
    let r1 = Receiver::load(&mut t, rcv1).unwrap();
    assert_eq!((r1.attr.status, r1.attr.next), (RcvStatus::Online, rcv2));
    let last_in_old = r1.entries(&mut t).unwrap().pop().unwrap();
    assert_eq!(last_in_old.type_name(), "J NR");
    assert_eq!(last_in_old.object.as_ref().unwrap().object, name("RCV0002"));
    let first_in_new = &r2.entries(&mut t).unwrap()[0];
    assert_eq!(first_in_new.type_name(), "J PR");
    assert_eq!(
        first_in_new.object.as_ref().unwrap().object,
        name("RCV0001")
    );
    assert_eq!(first_in_new.seq, last_in_old.seq + 1);
    let dir = j.receivers(&mut t).unwrap();
    assert_eq!(dir.len(), 2);
    assert_eq!(
        (dir[0].status, dir[1].status),
        (RcvStatus::Online, RcvStatus::Attached)
    );
    // Sequence numbers are contiguous across the chain.
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    let seqs: Vec<u64> = all.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());
    assert_eq!(all.len(), 2 + 6 + 2);
    // Filters.
    let f = Filter {
        codes: vec![code::JOURNAL],
        ..Filter::default()
    };
    let js = dspjrn(&mut t, jid, &f).unwrap();
    assert_eq!(
        js.iter().map(Entry::type_name).collect::<Vec<_>>(),
        ["J NR", "J PR"]
    );
    let f = Filter {
        types: vec![*b"03"],
        job: Some(7),
        from_seq: Some(4),
        to_seq: Some(6),
        ..Filter::default()
    };
    let some = dspjrn(&mut t, jid, &f).unwrap();
    assert_eq!(some.iter().map(|e| e.seq).collect::<Vec<_>>(), [4, 5, 6]);
    let f = Filter {
        current_only: true,
        ..Filter::default()
    };
    let cur = dspjrn(&mut t, jid, &f).unwrap();
    assert!(cur.iter().all(|e| e.seq >= first_in_new.seq));
    let f = Filter {
        receivers: Some((rcv2, rcv1)),
        ..Filter::default()
    };
    assert!(matches!(
        dspjrn(&mut t, jid, &f),
        Err(JrnError::Range { .. })
    ));
    let f = Filter {
        from_time: Some(NOW + 5),
        to_time: Some(NOW + 5),
        ..Filter::default()
    };
    let timed = dspjrn(&mut t, jid, &f).unwrap();
    assert_eq!(timed.len(), 1);
    assert_eq!(timed[0].seq, 2);
    // The cursor seeks across receivers.
    let mut c = ChainCursor::new(&mut t, &j, &Filter::default()).unwrap();
    c.seek(&mut t, first_in_new.seq + 1).unwrap();
    assert_eq!(c.next(&mut t).unwrap().unwrap().seq, first_in_new.seq + 1);
    let text = render(&all);
    assert!(text.contains("J PR"));
    assert!(text.contains("000007/QSECOFR/QS400"));
    assert!(text.contains("QSTEST/RCV0001"));
    let rcv_text = dspjrnrcva(&mut t, rcv1).unwrap();
    assert!(rcv_text.contains("Status                          :  *ONLINE"));
    assert!(rcv_text.contains("Next receiver                   :  RCV0002"));
    assert!(rcv_text.contains("Journal                         :  QSTJRN"));
    // CHGJRN to a named receiver; a used one is refused.
    let rcv3 = crtjrnrcv(
        &mut t,
        lib,
        name("RCV0003"),
        &CrtJrnRcvOpts {
            now: NOW,
            ..CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    let mut j = Journal::load(&mut t, jid).unwrap();
    assert!(matches!(
        j.chgjrn(&mut t, NewReceiver::Named(rcv1), NOW + 10),
        Err(JrnError::ReceiverUsed { .. })
    ));
    assert_eq!(
        j.chgjrn(&mut t, NewReceiver::Named(rcv3), NOW + 10)
            .unwrap(),
        rcv3
    );
    assert_eq!(j.attr.attached, rcv3);
    assert_eq!(j.chain(&mut t).unwrap().len(), 3);
    // Deleting: the attached receiver is refused, a detached one unlinks.
    let e = dltjrnrcv(&mut t, JobHandle(1), rcv3).unwrap_err();
    assert!(matches!(e, JrnError::ReceiverAttached { .. }));
    assert_eq!(e.msgid(), Some("CPF7022"));
    dltjrnrcv(&mut t, JobHandle(1), rcv1).unwrap();
    let r2 = Receiver::load(&mut t, rcv2).unwrap();
    assert!(r2.attr.prev.is_none());
    assert!(Receiver::load(&mut t, rcv1).is_err());
    // The chain skips the gone receiver; the directory still lists it.
    assert_eq!(j.chain(&mut t).unwrap().len(), 2);
    assert_eq!(j.receivers(&mut t).unwrap().len(), 3);
    let rest = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    assert_eq!(rest[0].type_name(), "J PR");
    // DLTJRN waits for the journaled objects to go.
    j.add_object(
        &mut t,
        JrnObjEntry {
            object: rcv2,
            object_type: jrn::JRNRCV,
            both_images: true,
            omit_open_close: false,
            started: NOW,
        },
        "RCV0002",
    )
    .unwrap();
    assert!(matches!(
        j.add_object(
            &mut t,
            JrnObjEntry {
                object: rcv2,
                object_type: jrn::JRNRCV,
                both_images: true,
                omit_open_close: false,
                started: NOW,
            },
            "RCV0002",
        ),
        Err(JrnError::AlreadyJournaled { .. })
    ));
    assert!(j.object(&mut t, rcv2).unwrap().unwrap().both_images);
    let e = dltjrn(&mut t, JobHandle(1), jid, NOW + 20).unwrap_err();
    assert!(matches!(e, JrnError::ObjectsJournaled { count: 1, .. }));
    j.remove_object(&mut t, rcv2, "RCV0002").unwrap();
    assert!(matches!(
        j.remove_object(&mut t, rcv2, "RCV0002"),
        Err(JrnError::NotJournaled { .. })
    ));
    dltjrn(&mut t, JobHandle(1), jid, NOW + 20).unwrap();
    let r3 = Receiver::load(&mut t, rcv3).unwrap();
    assert_eq!(
        (r3.attr.status, r3.attr.detached),
        (RcvStatus::Online, NOW + 20)
    );
    dltjrnrcv(&mut t, JobHandle(1), rcv3).unwrap();
    t.commit().unwrap();
    // Everything survives a reopen.
    let mut t = store.begin().unwrap();
    assert!(Journal::load(&mut t, jid).is_err());
    let r2 = Receiver::load(&mut t, rcv2).unwrap();
    // J PR, the two user entries after the swap, J NR.
    assert_eq!(r2.entries(&mut t).unwrap().len(), 4);
}

#[test]
fn many_objects_grow_the_tables() {
    let (mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let rcv = crtjrnrcv(&mut t, lib, name("R"), &CrtJrnRcvOpts::default()).unwrap();
    let jid = crtjrn(&mut t, lib, name("J"), rcv, &CrtJrnOpts::default()).unwrap();
    let mut j = Journal::load(&mut t, jid).unwrap();
    for i in 0..20u64 {
        let fake = ObjectId {
            asp: 1,
            type_code: 0x0013,
            serial: 1000 + i,
            ..rcv
        };
        j.add_object(
            &mut t,
            JrnObjEntry {
                object: fake,
                object_type: 0x0013,
                both_images: false,
                omit_open_close: false,
                started: 0,
            },
            "F",
        )
        .unwrap();
    }
    assert_eq!(j.objects(&mut t).unwrap().len(), 20);
    assert!(j.attr.object_cap >= 20);
    for _ in 0..12 {
        j.chgjrn(&mut t, NewReceiver::Gen, 0).unwrap();
    }
    assert_eq!(j.receivers(&mut t).unwrap().len(), 13);
    assert_eq!(j.objects(&mut t).unwrap().len(), 20);
    let chain = j.chain(&mut t).unwrap();
    assert_eq!(chain.last().unwrap().name(), name("R0012"));
    t.commit().unwrap();
}
