// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journaling of physical files: `STRJRNPF`/`ENDJRNPF`, the entries
//! record operations, opens, utilities and member commands write.

#![allow(clippy::too_many_lines)]

mod common;

use common::{customer_def, name, open_new, record};
use db::file::{self, CreateFileOpts};
use db::journal::{endjrnpf, strjrnpf};
use db::odp::{self, Odp, OpenOpts, Usage};
use db::util::{CpyfOpts, KeyFile, MbrOpt, Target, clrpfm, cpyf, rgzpfm};
use db::{DbError, KeyArg, Member};
use jrn::journal::{CrtJrnOpts, Journal, crtjrn};
use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
use jrn::{Entry, Filter, RecordImage, dspjrn};
use qsname::MemberSpec;
use store::obj::JobHandle;

const NOW: u64 = 1_767_225_600;

fn types(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(Entry::type_name).collect()
}

#[test]
fn journaled_file_writes_entries() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &customer_def(),
        &CreateFileOpts {
            now: NOW,
            max_members: 0,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let rcv = crtjrnrcv(
        &mut t,
        lib,
        name("QSTRCV001"),
        &CrtJrnRcvOpts {
            now: NOW,
            ..CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    let jid = crtjrn(
        &mut t,
        lib,
        name("QSTJRN"),
        rcv,
        &CrtJrnOpts {
            now: NOW,
            ..CrtJrnOpts::default()
        },
    )
    .unwrap();
    // Not journaled yet: a write leaves no entry; ENDJRNPF is refused.
    Member::open_by(&mut t, cust, MemberSpec::First)
        .unwrap()
        .write_new(&mut t, &record(1, "Alice"), &[0])
        .unwrap();
    assert!(dspjrn(&mut t, jid, &Filter::default()).unwrap().is_empty());
    let e = endjrnpf(&mut t, JobHandle(1), cust, NOW).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF7032"));
    // STRJRNPF IMAGES(*BOTH): one F JM per member.
    strjrnpf(&mut t, JobHandle(1), cust, jid, true, false, NOW + 1).unwrap();
    let desc = file::describe(&mut t, cust).unwrap();
    assert_eq!(desc.attr.journal, jid);
    assert!(desc.attr.jrn_both_images && !desc.attr.jrn_omit_opnclo);
    let j = Journal::load(&mut t, jid).unwrap();
    let obj = j.object(&mut t, cust).unwrap().unwrap();
    assert!(obj.both_images && obj.started == NOW + 1);
    let e = strjrnpf(&mut t, JobHandle(1), cust, jid, false, false, NOW).unwrap_err();
    assert!(matches!(
        e,
        DbError::Journal(jrn::JrnError::AlreadyJournaled { .. })
    ));
    assert_eq!(e.msgid(), Some("CPF7030"));
    // Record operations through an open data path.
    let locks = odp::new_shared();
    let mut o = Odp::open(
        &mut t,
        locks.clone(),
        cust,
        OpenOpts {
            usage: Usage::ALL,
            ..OpenOpts::input(JobHandle(5))
        },
    )
    .unwrap();
    let rrn = o.write(&mut t, &record(2, "Bob"), &[0]).unwrap();
    assert_eq!(rrn, 2);
    o.chain(
        &mut t,
        &KeyArg::one(db::Value::Num("1".parse().unwrap())),
        true,
    )
    .unwrap();
    o.update(&mut t, &record(1, "Alicia"), &[0]).unwrap();
    o.chain(
        &mut t,
        &KeyArg::one(db::Value::Num("2".parse().unwrap())),
        true,
    )
    .unwrap();
    o.delete(&mut t).unwrap();
    o.close(&mut t);
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    assert_eq!(
        types(&all),
        ["F JM", "F OP", "R PT", "R UB", "R UP", "R DL", "F CL"]
    );
    assert_eq!(all[0].time, NOW + 1);
    let pt = &all[2];
    assert_eq!(pt.count, 2);
    assert_eq!(pt.job.number, 5);
    let o_ref = pt.object.as_ref().unwrap();
    assert_eq!(
        (o_ref.library, o_ref.object, o_ref.member),
        (name("QSTEST"), name("CUST"), Some(name("CUST")))
    );
    let img = RecordImage::decode(&pt.data).unwrap();
    assert_eq!((img.record, img.null_map), (record(2, "Bob"), vec![0]));
    assert_eq!(
        RecordImage::decode(&all[3].data).unwrap().record,
        record(1, "Alice")
    );
    assert_eq!(
        RecordImage::decode(&all[4].data).unwrap().record,
        record(1, "Alicia")
    );
    assert_eq!(all[3].count, 1);
    let dl = RecordImage::decode(&all[5].data).unwrap();
    assert_eq!((all[5].count, dl.record), (2, record(2, "Bob")));
    assert_eq!(all[6].job.number, 5);
    // Utilities and member commands.
    file::add_member(&mut t, cust, name("ARCH"), "", None, NOW).unwrap();
    cpyf(
        &mut t,
        JobHandle(6),
        cust,
        &Target::File(cust),
        &CpyfOpts {
            to_member: db::util::ToMember::Named(name("ARCH")),
            mbropt: MbrOpt::Add,
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    rgzpfm(
        &mut t,
        JobHandle(6),
        cust,
        MemberSpec::First,
        &KeyFile::None,
    )
    .unwrap();
    clrpfm(&mut t, JobHandle(6), cust, MemberSpec::Name(name("ARCH"))).unwrap();
    file::remove_member(&mut t, JobHandle(6), cust, name("ARCH")).unwrap();
    let since = Filter {
        from_seq: Some(8),
        ..Filter::default()
    };
    let later = dspjrn(&mut t, jid, &since).unwrap();
    assert_eq!(
        types(&later),
        ["F MA", "R PT", "F MC", "R PT", "F MR", "F MC", "F MD"]
    );
    assert_eq!(later[0].object.as_ref().unwrap().member, Some(name("ARCH")));
    assert_eq!(later[1].object.as_ref().unwrap().member, Some(name("ARCH")));
    assert_eq!(later[1].job.number, 6);
    // RGZPFM cleared and rewrote CUST: the PT is the surviving record.
    assert_eq!(later[3].object.as_ref().unwrap().member, Some(name("CUST")));
    assert_eq!(
        RecordImage::decode(&later[3].data).unwrap().record,
        record(1, "Alicia")
    );
    // Filters by member and by record.
    let arch = Filter {
        object: Some((None, name("CUST"), Some(name("ARCH")))),
        ..Filter::default()
    };
    assert_eq!(
        types(&dspjrn(&mut t, jid, &arch).unwrap()),
        ["F MA", "R PT", "F MC", "F MD"]
    );
    // OMTJRNE(*OPNCLO): no open and close entries.
    endjrnpf(&mut t, JobHandle(1), cust, NOW + 2).unwrap();
    let desc = file::describe(&mut t, cust).unwrap();
    assert!(desc.attr.journal.is_none());
    assert!(
        Journal::load(&mut t, jid)
            .unwrap()
            .object(&mut t, cust)
            .unwrap()
            .is_none()
    );
    let tail = dspjrn(&mut t, jid, &Filter::default())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!((tail.type_name().as_str(), tail.time), ("F EJ", NOW + 2));
    strjrnpf(&mut t, JobHandle(1), cust, jid, false, true, NOW + 3).unwrap();
    let before = dspjrn(&mut t, jid, &Filter::default()).unwrap().len();
    let mut o = Odp::open(
        &mut t,
        locks,
        cust,
        OpenOpts {
            usage: Usage::ALL,
            ..OpenOpts::input(JobHandle(5))
        },
    )
    .unwrap();
    o.write(&mut t, &record(9, "Zed"), &[0]).unwrap();
    o.chain(
        &mut t,
        &KeyArg::one(db::Value::Num("9".parse().unwrap())),
        true,
    )
    .unwrap();
    o.update(&mut t, &record(9, "Zoe"), &[0]).unwrap();
    o.close(&mut t);
    let after = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    assert_eq!(types(&after[before..]), ["R PT", "R UP"]);
    t.commit().unwrap();
    // Entries survive a reopen of the store.
    let mut t = store.begin().unwrap();
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    assert_eq!(all.len(), before + 2);
    assert!(all.windows(2).all(|w| w[1].seq == w[0].seq + 1));
}
