// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commitment control: cycles, commit, rollback from before images,
//! locks held to the boundary, `APYJRNCHG`/`RMVJRNCHG`, and recovery
//! of a cycle left open by a crash.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{customer_def, name, open_new, record, reopen};
use db::cmt::{CmtScope, LckLvl};
use db::file::{self, CreateFileOpts};
use db::journal::{JrnRange, apyjrnchg, recover_all, recover_journal, rmvjrnchg, strjrnpf};
use db::odp::{self, OpenOpts, Session, Usage, session::resolve_in};
use db::{DbError, KeyArg, Member, Value};
use dds::FileRef;
use jrn::journal::{CrtJrnOpts, crtjrn};
use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
use jrn::{Entry, Filter, dspjrn};
use qsname::MemberSpec;
use store::Store;
use store::obj::{JobHandle, ObjectId};

const NOW: u64 = 1_767_225_600;

fn key(n: &str) -> KeyArg {
    KeyArg::one(Value::Num(n.parse().unwrap()))
}

/// CUST with Alice (1), Bob (2), Carol (3), journaled to QSTJRN.
fn setup(store: &mut Store, lib: ObjectId, both: bool) -> (ObjectId, ObjectId) {
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &customer_def(),
        &CreateFileOpts {
            now: NOW,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    {
        let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
        for (n, w) in [(1, "Alice"), (2, "Bob"), (3, "Carol")] {
            m.write_new(&mut t, &record(n, w), &[0]).unwrap();
        }
    }
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
    strjrnpf(&mut t, JobHandle(1), cust, jid, both, true, NOW).unwrap();
    t.commit().unwrap();
    (cust, jid)
}

fn names(store: &mut Store, cust: ObjectId) -> Vec<(u64, String)> {
    let mut t = store.begin().unwrap();
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let fmt = m.format().clone();
    let (ni, nf) = fmt.format.field(&name("NAME")).unwrap();
    m.scan(&mut t, 1, 100)
        .unwrap()
        .iter()
        .map(|s| {
            let v = db::record::read_field(nf, &s.record, &s.null_map, fmt.null_index(ni)).unwrap();
            match v {
                Value::Char(b) => (s.rrn, ccsid::decode_trimmed(&b)),
                v => panic!("{v:?}"),
            }
        })
        .collect()
}

#[test]
fn commit_and_rollback() {
    let (_d, mut store, lib) = open_new();
    let (cust, jid) = setup(&mut store, lib, false);
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks.clone());
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("CUST"), cust)]);
    let fr = FileRef::parse("CUST").unwrap();
    let with_commit = OpenOpts {
        usage: Usage::ALL,
        commit: true,
        ..OpenOpts::input(JobHandle(0))
    };
    // COMMIT(*YES) needs STRCMTCTL first; COMMIT needs it too.
    let mut t = store.begin().unwrap();
    let e = s
        .open(&mut t, &fr, &mut resolve, with_commit.clone())
        .unwrap_err();
    assert!(matches!(e, DbError::CommitOpen));
    assert_eq!(e.msgid(), Some("CPF4293"));
    assert!(matches!(
        s.commit(&mut t, None),
        Err(DbError::CommitNotActive)
    ));
    s.strcmtctl(&mut t, LckLvl::Chg, Some(jid), NOW).unwrap();
    assert!(matches!(
        s.strcmtctl(&mut t, LckLvl::Chg, None, NOW),
        Err(DbError::CommitActive)
    ));
    // The default journal got C SC.
    let last = dspjrn(&mut t, jid, &Filter::default())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(last.type_name(), "C SC");
    let o = s
        .open(&mut t, &fr, &mut resolve, with_commit.clone())
        .unwrap();
    // A cycle: write Dave, update Bob, delete Carol, then roll it back.
    {
        let mut o = o.borrow_mut();
        o.write(&mut t, &record(4, "Dave"), &[0]).unwrap();
        o.chain(&mut t, &key("2"), true).unwrap();
        o.update(&mut t, &record(2, "Bobby"), &[0]).unwrap();
        o.chain(&mut t, &key("3"), true).unwrap();
        o.delete(&mut t).unwrap();
    }
    let c = s.commit_def().unwrap().borrow();
    assert_ne!(c.cycle(), 0);
    assert_eq!(c.changes(), 3);
    // Changed records stay locked: another job cannot take them.
    assert_eq!(c.held().len(), 3);
    drop(c);
    let member = o.borrow().member().id();
    assert_eq!(locks.lock().unwrap().holder(member, 2), Some(JobHandle(1)));
    assert_eq!(locks.lock().unwrap().holder(member, 4), Some(JobHandle(1)));
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [(1, "Alice".into()), (2, "Bobby".into()), (4, "Dave".into())]
    );
    let mut t = store.begin().unwrap();
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    let cycle = s.commit_def().unwrap().borrow().cycle();
    let mine: Vec<&Entry> = all.iter().filter(|e| e.commit_cycle == cycle).collect();
    assert_eq!(
        mine.iter().map(|e| e.type_name()).collect::<Vec<_>>(),
        ["C BC", "R PT", "R UB", "R UP", "R DL"]
    );
    // The before image was journaled although IMAGES(*AFTER).
    let r = s.rollbck(&mut t, Some(NOW + 10)).unwrap();
    assert_eq!((r.applied, r.skipped), (3, 1));
    assert!(locks.lock().unwrap().is_empty());
    assert_eq!(s.commit_def().unwrap().borrow().cycle(), 0);
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [(1, "Alice".into()), (2, "Bob".into()), (3, "Carol".into())]
    );
    let mut t = store.begin().unwrap();
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    let undo: Vec<String> = all
        .iter()
        .filter(|e| e.commit_cycle == cycle && e.seq > mine.last().unwrap().seq)
        .map(Entry::type_name)
        .collect();
    assert_eq!(undo, ["R PT", "R BR", "R UR", "R DR", "C RB"]);
    assert!(
        all.iter()
            .filter(|e| e.type_name() == "R DR")
            .all(|e| e.flags & jrn::entry::flags::ROLLBACK != 0)
    );
    // The path still works after the rollback: Carol is back, Dave gone.
    {
        let mut o = o.borrow_mut();
        assert!(o.chain(&mut t, &key("3"), false).unwrap().is_some());
        assert!(o.chain(&mut t, &key("4"), false).unwrap().is_none());
        // A second cycle, committed.
        o.write(&mut t, &record(5, "Erin"), &[0]).unwrap();
        o.chain(&mut t, &key("1"), true).unwrap();
        o.update(&mut t, &record(1, "Alicia"), &[0]).unwrap();
    }
    let second = s.commit_def().unwrap().borrow().cycle();
    assert!(second > cycle);
    assert_eq!(s.commit(&mut t, Some(NOW + 20)).unwrap(), second);
    assert!(locks.lock().unwrap().is_empty());
    let tail = dspjrn(&mut t, jid, &Filter::default())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (tail.type_name().as_str(), tail.commit_cycle),
        ("C CM", second)
    );
    // COMMIT with nothing open is fine; ENDCMTCTL rolls an open cycle back.
    assert_eq!(s.commit(&mut t, None).unwrap(), 0);
    o.borrow_mut()
        .write(&mut t, &record(6, "Fay"), &[0])
        .unwrap();
    assert!(matches!(
        s.endcmtctl(&mut t, None),
        Err(DbError::CommitFilesOpen)
    ));
    s.close(&mut t, &o);
    let r = s.endcmtctl(&mut t, Some(NOW + 30)).unwrap();
    assert_eq!(r.applied, 1);
    assert!(s.commit_def().is_none());
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [
            (1, "Alicia".into()),
            (2, "Bob".into()),
            (3, "Carol".into()),
            (5, "Erin".into())
        ]
    );
    // A file without a journal cannot change under commitment control.
    let mut t = store.begin().unwrap();
    let plain = file::create_pf(
        &mut t,
        lib,
        name("PLAIN"),
        &customer_def(),
        &CreateFileOpts {
            now: NOW,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("PLAIN"), plain)]);
    s.strcmtctl(&mut t, LckLvl::All, None, NOW).unwrap();
    let p = s
        .open(
            &mut t,
            &FileRef::parse("PLAIN").unwrap(),
            &mut resolve,
            with_commit,
        )
        .unwrap();
    let e = p
        .borrow_mut()
        .write(&mut t, &record(1, "X"), &[0])
        .unwrap_err();
    assert!(matches!(e, DbError::CommitNeedsJournal { .. }));
    assert_eq!(e.msgid(), Some("CPF8351"));
    assert_eq!(p.borrow().fb.msgid, Some("CPF8351"));
    s.end(&mut t);
    t.commit().unwrap();
}

#[test]
fn lock_levels() {
    let (_d, mut store, lib) = open_new();
    let (cust, _jid) = setup(&mut store, lib, false);
    let locks = odp::new_shared();
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("CUST"), cust)]);
    let fr = FileRef::parse("CUST").unwrap();
    let mut t = store.begin().unwrap();
    let cc = OpenOpts {
        usage: Usage::UPDATE,
        commit: true,
        ..OpenOpts::input(JobHandle(0))
    };
    // *ALL: every record read stays locked until the commit.
    let mut s = Session::new(JobHandle(1), locks.clone());
    s.strcmtctl(&mut t, LckLvl::All, None, NOW).unwrap();
    let o = s.open(&mut t, &fr, &mut resolve, cc.clone()).unwrap();
    {
        let mut o = o.borrow_mut();
        o.read(&mut t, false).unwrap();
        o.read(&mut t, false).unwrap();
    }
    let member = o.borrow().member().id();
    assert_eq!(locks.lock().unwrap().holder(member, 1), Some(JobHandle(1)));
    assert_eq!(locks.lock().unwrap().holder(member, 2), Some(JobHandle(1)));
    // Another job is refused the locked record, allowed the free one.
    let mut other = Session::new(JobHandle(2), locks.clone());
    let b = other
        .open(
            &mut t,
            &fr,
            &mut resolve,
            OpenOpts {
                usage: Usage::UPDATE,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .unwrap();
    assert!(matches!(
        b.borrow_mut().chain(&mut t, &key("1"), true),
        Err(DbError::RecordLocked { holder: 1, .. })
    ));
    assert!(
        b.borrow_mut()
            .chain(&mut t, &key("3"), true)
            .unwrap()
            .is_some()
    );
    b.borrow_mut().unlock_op(&mut t).unwrap();
    s.commit(&mut t, None).unwrap();
    assert!(locks.lock().unwrap().is_empty());
    // ENDCMTCTL needs the files under commitment control closed.
    assert!(matches!(
        s.endcmtctl(&mut t, None),
        Err(DbError::CommitFilesOpen)
    ));
    s.close(&mut t, &o);
    s.endcmtctl(&mut t, None).unwrap();
    // *CS: a read locks until the next read; a change keeps its lock.
    s.strcmtctl(&mut t, LckLvl::Cs, None, NOW).unwrap();
    let o = s.open(&mut t, &fr, &mut resolve, cc.clone()).unwrap();
    {
        let mut o = o.borrow_mut();
        o.setll(&mut t, &KeyArg::Start).unwrap();
        o.read(&mut t, false).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 1), Some(JobHandle(1)));
        o.read(&mut t, false).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 1), None);
        assert_eq!(locks.lock().unwrap().holder(member, 2), Some(JobHandle(1)));
        o.update(&mut t, &record(2, "Bobby"), &[0]).unwrap();
        o.read(&mut t, false).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 2), Some(JobHandle(1)));
        assert_eq!(locks.lock().unwrap().holder(member, 3), Some(JobHandle(1)));
    }
    s.rollbck(&mut t, None).unwrap();
    assert!(locks.lock().unwrap().is_empty());
    s.close(&mut t, &o);
    s.endcmtctl(&mut t, None).unwrap();
    // *CHG: a plain read takes no lock; a read for update does, released
    // when nothing changes.
    s.strcmtctl(&mut t, LckLvl::Chg, None, NOW).unwrap();
    let o = s.open(&mut t, &fr, &mut resolve, cc).unwrap();
    {
        let mut o = o.borrow_mut();
        o.setll(&mut t, &KeyArg::Start).unwrap();
        o.read(&mut t, false).unwrap();
        assert!(locks.lock().unwrap().is_empty());
        o.read(&mut t, true).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 2), Some(JobHandle(1)));
        o.read(&mut t, true).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 2), None);
        o.update(&mut t, &record(3, "Carole"), &[0]).unwrap();
        assert_eq!(locks.lock().unwrap().holder(member, 3), Some(JobHandle(1)));
    }
    s.commit(&mut t, None).unwrap();
    assert!(locks.lock().unwrap().is_empty());
    s.close(&mut t, &o);
    other.close(&mut t, &b);
    s.end(&mut t);
    other.end(&mut t);
    t.commit().unwrap();
    assert_eq!(names(&mut store, cust)[2], (3, "Carole".into()));
}

#[test]
fn apply_remove_and_recover() {
    let (d, mut store, lib) = open_new();
    let (cust, jid) = setup(&mut store, lib, true);
    // Plain changes: write Dave, update Bob, delete Alice.
    let mut t = store.begin().unwrap();
    {
        let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
        m.jrn.job = JobHandle(3);
        m.write_new(&mut t, &record(4, "Dave"), &[0]).unwrap();
        m.update(&mut t, 2, &record(2, "Bobby"), &[0]).unwrap();
        m.delete(&mut t, 1).unwrap();
    }
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    let first_change = all.iter().find(|e| e.type_name() == "R PT").unwrap().seq;
    // RMVJRNCHG back to before the changes.
    let range = JrnRange {
        from_seq: Some(first_change),
        ..JrnRange::default()
    };
    let r = rmvjrnchg(&mut t, JobHandle(9), jid, None, &range, false, NOW + 5).unwrap();
    assert_eq!((r.applied, r.skipped), (3, 1));
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [(1, "Alice".into()), (2, "Bob".into()), (3, "Carol".into())]
    );
    // The undo wrote flagged entries that a later apply ignores.
    let mut t = store.begin().unwrap();
    let all = dspjrn(&mut t, jid, &Filter::default()).unwrap();
    let flagged: Vec<String> = all
        .iter()
        .filter(|e| e.flags & jrn::entry::flags::APPLIED != 0)
        .map(Entry::type_name)
        .collect();
    assert_eq!(flagged, ["R PT", "R BR", "R UR", "R DR"]);
    // APYJRNCHG replays the original changes forward.
    let r = apyjrnchg(
        &mut t,
        JobHandle(9),
        jid,
        Some(&[cust]),
        &range,
        false,
        NOW + 6,
    )
    .unwrap();
    assert_eq!(r.applied, 3);
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [(2, "Bobby".into()), (3, "Carol".into()), (4, "Dave".into())]
    );
    // A cycle left open by a crash: changes under commitment control,
    // the job vanishes, the store reopens, recovery rolls it back.
    let mut t = store.begin().unwrap();
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks);
    s.strcmtctl(&mut t, LckLvl::Chg, None, NOW).unwrap();
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("CUST"), cust)]);
    let o = s
        .open(
            &mut t,
            &FileRef::parse("CUST").unwrap(),
            &mut resolve,
            OpenOpts {
                usage: Usage::ALL,
                commit: true,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .unwrap();
    {
        let mut o = o.borrow_mut();
        o.write(&mut t, &record(7, "Gus"), &[0]).unwrap();
        o.chain(&mut t, &key("3"), true).unwrap();
        o.delete(&mut t).unwrap();
    }
    let cycle = s.commit_def().unwrap().borrow().cycle();
    t.commit().unwrap();
    drop(s);
    drop(o);
    let mut store = reopen(&d);
    assert_eq!(
        names(&mut store, cust),
        [(2, "Bobby".into()), (4, "Dave".into()), (5, "Gus".into())]
    );
    let mut t = store.begin().unwrap();
    let qsys = file::describe(&mut t, cust).unwrap().header.library;
    let qsys = store::obj::ops::read_header(&mut t, qsys)
        .unwrap()
        .1
        .library;
    let r = recover_all(&mut t, qsys, NOW + 100).unwrap();
    assert_eq!(
        (r.journals, r.cycles_rolled_back, r.records_undone),
        (1, 1, 2)
    );
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [(2, "Bobby".into()), (3, "Carol".into()), (4, "Dave".into())]
    );
    let mut t = store.begin().unwrap();
    let tail = dspjrn(&mut t, jid, &Filter::default())
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        (tail.type_name().as_str(), tail.commit_cycle, tail.time),
        ("C RB", cycle, NOW + 100)
    );
    // Running again finds nothing open.
    let r = recover_journal(&mut t, jid, NOW + 101).unwrap();
    assert_eq!(r.cycles_rolled_back, 0);
    // CMTBDY(*YES): a cycle without its commit in range is left alone.
    let r = rmvjrnchg(
        &mut t,
        JobHandle(9),
        jid,
        None,
        &JrnRange {
            from_seq: Some(tail.seq - 3),
            to_seq: Some(tail.seq - 1),
            ..JrnRange::default()
        },
        true,
        NOW + 102,
    )
    .unwrap();
    assert_eq!(r.applied, 0);
    t.commit().unwrap();
}

#[test]
fn activation_group_scope() {
    let (_d, mut store, lib) = open_new();
    let (cust, jid) = setup(&mut store, lib, true);
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks.clone());
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("CUST"), cust)]);
    let fr = FileRef::parse("CUST").unwrap();
    let with_commit = OpenOpts {
        usage: Usage::ALL,
        commit: true,
        ..OpenOpts::input(JobHandle(0))
    };
    let mut t = store.begin().unwrap();
    // The job's definition, then one of the group's own.
    s.strcmtctl(&mut t, LckLvl::Chg, Some(jid), NOW).unwrap();
    let job_def = s.commit_def().unwrap().clone();
    s.set_opener("GRP1");
    assert!(
        std::rc::Rc::ptr_eq(s.commit_def().unwrap(), &job_def),
        "no own definition: the job's"
    );
    s.strcmtctl_in(
        &mut t,
        LckLvl::All,
        None,
        NOW,
        CmtScope::ActGrp("GRP1".into()),
    )
    .unwrap();
    assert!(matches!(
        s.strcmtctl_in(
            &mut t,
            LckLvl::All,
            None,
            NOW,
            CmtScope::ActGrp("GRP1".into())
        ),
        Err(DbError::CommitActive)
    ));
    assert!(!std::rc::Rc::ptr_eq(s.commit_def().unwrap(), &job_def));
    assert_eq!(s.commit_def().unwrap().borrow().lcklvl(), LckLvl::All);
    // A change from the group goes into the group's cycle, not the job's.
    let o = s
        .open(&mut t, &fr, &mut resolve, with_commit.clone())
        .unwrap();
    o.borrow_mut()
        .write(&mut t, &record(4, "Dave"), &[0])
        .unwrap();
    assert_eq!(s.commit_def().unwrap().borrow().changes(), 1);
    assert_eq!(job_def.borrow().changes(), 0);
    // ENDCMTCTL from the group is refused while its file is open; the
    // job's definition is untouched by it.
    assert!(matches!(
        s.endcmtctl(&mut t, None),
        Err(DbError::CommitFilesOpen)
    ));
    s.close(&mut t, &o);
    let r = s.endcmtctl(&mut t, Some(NOW + 5)).unwrap();
    assert_eq!(r.applied, 1, "the open cycle was rolled back");
    assert!(std::rc::Rc::ptr_eq(s.commit_def().unwrap(), &job_def));
    // Back at the default group the job's definition still runs.
    s.set_opener("*DFTACTGRP");
    let o = s.open(&mut t, &fr, &mut resolve, with_commit).unwrap();
    o.borrow_mut()
        .write(&mut t, &record(5, "Eve"), &[0])
        .unwrap();
    assert_eq!(job_def.borrow().changes(), 1);
    s.commit(&mut t, Some(NOW + 6)).unwrap();
    s.close(&mut t, &o);
    // A group that ends takes its definition with it, rolled back.
    s.set_opener("GRP2");
    s.strcmtctl_in(
        &mut t,
        LckLvl::Chg,
        None,
        NOW,
        CmtScope::ActGrp("GRP2".into()),
    )
    .unwrap();
    let o = s
        .open(
            &mut t,
            &fr,
            &mut resolve,
            OpenOpts {
                usage: Usage::ALL,
                commit: true,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .unwrap();
    o.borrow_mut()
        .write(&mut t, &record(6, "Fay"), &[0])
        .unwrap();
    s.close(&mut t, &o);
    s.reclaim_in(&mut t, "GRP2");
    assert!(std::rc::Rc::ptr_eq(s.commit_def().unwrap(), &job_def));
    s.end(&mut t);
    t.commit().unwrap();
    assert_eq!(
        names(&mut store, cust),
        [
            (1, "Alice".into()),
            (2, "Bob".into()),
            (3, "Carol".into()),
            (5, "Eve".into())
        ]
    );
}
