// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record-level access through the open data path: positioning, reads
//! in both directions, equal-key reads, random access by key and RRN,
//! writes, updates and deletes with their lock rules, level checks and
//! the status codes RPG sees.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{customer_def, fld, key, lf, name, open_new, pf, rec, record, record_with, sel};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts, Usage, status};
use db::{DbError, KeyArg, Value};
use dds::MapRefs;
use qdecimal::Decimal;
use qsname::MemberSpec;
use store::obj::{JobHandle, ObjectId};

fn num(n: &str) -> Value {
    Value::Num(Decimal::parse(n).unwrap())
}

fn chars(s: &str) -> Value {
    Value::Char(ccsid::to_ebcdic(s).unwrap())
}

fn cusno(o: &Odp, r: &odp::Record) -> i64 {
    match o.field(r, "CUSNO").unwrap() {
        Value::Num(d) => i64::try_from(d.to_i128().unwrap()).unwrap(),
        v => panic!("{v:?}"),
    }
}

fn who(o: &Odp, r: &odp::Record) -> String {
    match o.field(r, "NAME").unwrap() {
        Value::Char(b) => ccsid::decode_trimmed(&b),
        v => panic!("{v:?}"),
    }
}

/// CUST with five customers; keys 1, 3, 3, 7, 10 (RRNs 1..=5).
fn setup(store: &mut store::Store, lib: ObjectId) -> ObjectId {
    let def = pf(&[
        rec("CUSREC", "TEXT('Customer')"),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer')"),
        fld("NAME", "30", "A", "", "TEXT('Name')"),
        fld("BAL", "9", "P", "2", "DFT(0)"),
        fld("SINCE", "", "L", "", "ALWNULL"),
        fld("NOTE", "100", "A", "", "VARLEN(20) ALWNULL"),
        key("CUSNO", ""),
        key("NAME", ""),
    ]);
    let mut t = store.begin().unwrap();
    let id = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &def,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut m = db::Member::open_by(&mut t, id, MemberSpec::First).unwrap();
    for (n, w) in [
        (1, "Alice"),
        (3, "Carol"),
        (3, "Bob"),
        (7, "Grace"),
        (10, "Judy"),
    ] {
        m.write_new(&mut t, &record(n, w), &[0]).unwrap();
    }
    t.commit().unwrap();
    id
}

#[test]
fn positioning_and_sequential_reads() {
    let (_d, mut store, lib) = open_new();
    let file = setup(&mut store, lib);
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let mut o = Odp::open(&mut t, locks.clone(), file, OpenOpts::input(JobHandle(1))).unwrap();
    assert_eq!(o.fb.status_code(), "00000");
    // READ to end of file.
    let mut seen = Vec::new();
    while let Some(r) = o.read(&mut t, true).unwrap() {
        seen.push((cusno(&o, &r), who(&o, &r), r.rrn));
    }
    assert_eq!(
        seen,
        [
            (1, "Alice".into(), 1),
            (3, "Bob".into(), 3),
            (3, "Carol".into(), 2),
            (7, "Grace".into(), 4),
            (10, "Judy".into(), 5)
        ]
    );
    assert!(o.fb.eof);
    assert_eq!(o.fb.status, status::EOF);
    assert_eq!(o.fb.status_code(), "00011");
    assert!(o.read(&mut t, true).unwrap().is_none());
    // Backward from the end.
    let r = o.readp(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 10);
    let r = o.readp(&mut t, true).unwrap().unwrap();
    assert_eq!(who(&o, &r), "Grace");
    // SETLL partial key, %EQUAL and %FOUND, then READE loop on the prefix.
    o.setll(&mut t, &KeyArg::one(num("3"))).unwrap();
    assert!(o.fb.found && o.fb.equal);
    let mut equal = Vec::new();
    while let Some(r) = o.reade(&mut t, Some(&KeyArg::one(num("3"))), true).unwrap() {
        equal.push(who(&o, &r));
    }
    assert_eq!(equal, ["Bob", "Carol"]);
    assert_eq!(o.fb.status, status::EOF);
    // After the failed READE the file is at its end until positioned
    // again: the next READ is end of file, as on the machine.
    assert!(o.read(&mut t, true).unwrap().is_none());
    assert_eq!(o.fb.status, status::EOF);
    // SETLL with a key between records: found but not equal.
    o.setll(&mut t, &KeyArg::one(num("5"))).unwrap();
    assert!(o.fb.found && !o.fb.equal);
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 7);
    // SETGT skips every record with the prefix.
    o.setgt(&mut t, &KeyArg::one(num("3"))).unwrap();
    assert!(o.fb.found);
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 7);
    o.setgt(&mut t, &KeyArg::one(num("10"))).unwrap();
    assert!(!o.fb.found);
    assert_eq!(o.fb.status, status::NOT_FOUND);
    assert!(o.read(&mut t, true).unwrap().is_none());
    // Full two-field key, READE without a key repeats the last key.
    o.setll(&mut t, &KeyArg::Values(vec![num("3"), chars("Bob")]))
        .unwrap();
    assert!(o.fb.equal);
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(who(&o, &r), "Bob");
    assert!(o.reade(&mut t, None, true).unwrap().is_none());
    // READPE backward over the prefix.
    o.setll(&mut t, &KeyArg::one(num("7"))).unwrap();
    let r = o
        .readpe(&mut t, Some(&KeyArg::one(num("3"))), true)
        .unwrap()
        .unwrap();
    assert_eq!(who(&o, &r), "Carol");
    let r = o
        .readpe(&mut t, Some(&KeyArg::one(num("3"))), true)
        .unwrap()
        .unwrap();
    assert_eq!(who(&o, &r), "Bob");
    assert!(
        o.readpe(&mut t, Some(&KeyArg::one(num("3"))), true)
            .unwrap()
            .is_none()
    );
    // *START, *END, *HIVAL, *LOVAL.
    o.setll(&mut t, &KeyArg::End).unwrap();
    let r = o.readp(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 10);
    o.setll(&mut t, &KeyArg::LoVal).unwrap();
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 1);
    o.setgt(&mut t, &KeyArg::HiVal).unwrap();
    assert!(!o.fb.found);
    // Too many key fields.
    assert!(matches!(
        o.setll(
            &mut t,
            &KeyArg::Values(vec![num("1"), chars("x"), num("9")])
        ),
        Err(DbError::Definition { .. })
    ));
    // Input-only: no locks taken.
    assert!(locks.lock().unwrap().is_empty());
    o.close(&mut t);
    t.commit().unwrap();
}

#[test]
fn random_access_write_update_delete() {
    let (_d, mut store, lib) = open_new();
    let file = setup(&mut store, lib);
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let mut o = Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            usage: Usage::ALL,
            ..OpenOpts::input(JobHandle(1))
        },
    )
    .unwrap();
    // CHAIN by key, found and not found.
    let r = o
        .chain(&mut t, &KeyArg::one(num("7")), true)
        .unwrap()
        .unwrap();
    assert_eq!(who(&o, &r), "Grace");
    assert_eq!(o.fb.rrn, 4);
    assert!(o.fb.found);
    assert_eq!(locks.lock().unwrap().len(), 1);
    assert!(
        o.chain(&mut t, &KeyArg::one(num("8")), true)
            .unwrap()
            .is_none()
    );
    assert_eq!(o.fb.status, status::NOT_FOUND);
    assert_eq!(o.fb.status_code(), "00012");
    assert!(!o.fb.found);
    // A not-found CHAIN released the lock and positioned where 8 would be.
    assert!(locks.lock().unwrap().is_empty());
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(cusno(&o, &r), 10);
    // UPDATE without a locked read is 01221.
    o.unlock_op(&mut t).unwrap();
    assert!(matches!(
        o.update(&mut t, &record(10, "x"), &[0]),
        Err(DbError::NoLockedRecord { .. })
    ));
    assert_eq!(o.fb.status, status::NO_PRIOR_READ);
    assert_eq!(o.fb.status_code(), "01221");
    // CHAIN with lock, UPDATE moving the key, lock released after.
    let r = o
        .chain(&mut t, &KeyArg::Values(vec![num("3"), chars("Bob")]), true)
        .unwrap()
        .unwrap();
    assert_eq!(r.rrn, 3);
    o.update(&mut t, &record_with(3, "Bobby", "1.00"), &[0])
        .unwrap();
    assert_eq!(o.fb.status, status::OK);
    assert!(locks.lock().unwrap().is_empty());
    // The next READ continues after the old key (3, Bob): Bobby sorts after Bob.
    let r = o.read(&mut t, true).unwrap().unwrap();
    assert_eq!(who(&o, &r), "Bobby");
    // A READ with the (N) extender takes no lock; UPDATE then fails.
    let r = o
        .chain(&mut t, &KeyArg::one(num("1")), false)
        .unwrap()
        .unwrap();
    assert_eq!(who(&o, &r), "Alice");
    assert!(locks.lock().unwrap().is_empty());
    assert!(matches!(
        o.update(&mut t, &record(1, "Al"), &[0]),
        Err(DbError::NoLockedRecord { .. })
    ));
    // WRITE, then CHAIN finds it; duplicate keys are allowed here (no UNIQUE).
    let rrn = o.write(&mut t, &record(5, "Eve"), &[0]).unwrap();
    assert_eq!(rrn, 6);
    assert_eq!(o.fb.rrn, 6);
    let r = o
        .chain(&mut t, &KeyArg::one(num("5")), true)
        .unwrap()
        .unwrap();
    assert_eq!(who(&o, &r), "Eve");
    // DELETE the locked record, then by key.
    o.delete(&mut t).unwrap();
    assert!(
        o.chain(&mut t, &KeyArg::one(num("5")), true)
            .unwrap()
            .is_none()
    );
    assert!(o.delete_key(&mut t, &KeyArg::one(num("10"))).unwrap());
    assert!(!o.delete_key(&mut t, &KeyArg::one(num("10"))).unwrap());
    // DELETE without a read for update.
    assert!(matches!(
        o.delete(&mut t),
        Err(DbError::NoLockedRecord { .. })
    ));
    // CHAIN by RRN works on a keyed open too.
    let r = o.chain(&mut t, &KeyArg::Rrn(1), true).unwrap().unwrap();
    assert_eq!(who(&o, &r), "Alice");
    assert!(o.chain(&mut t, &KeyArg::Rrn(5), true).unwrap().is_none());
    assert_eq!(o.fb.status, status::NOT_FOUND);
    assert!(o.chain(&mut t, &KeyArg::Rrn(99), true).unwrap().is_none());
    // Remaining in key order.
    o.setll(&mut t, &KeyArg::Start).unwrap();
    let mut left = Vec::new();
    while let Some(r) = o.read(&mut t, false).unwrap() {
        left.push(who(&o, &r));
    }
    assert_eq!(left, ["Alice", "Bobby", "Carol", "Grace"]);
    o.feod(&mut t).unwrap();
    o.close(&mut t);
    t.commit().unwrap();
}

#[test]
fn arrival_sequence_and_rrn() {
    let (_d, mut store, lib) = open_new();
    let file = setup(&mut store, lib);
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let mut o = Odp::open(
        &mut t,
        locks,
        file,
        OpenOpts {
            keyed: false,
            usage: Usage::UPDATE,
            ..OpenOpts::input(JobHandle(1))
        },
    )
    .unwrap();
    let mut seen = Vec::new();
    while let Some(r) = o.read(&mut t, true).unwrap() {
        seen.push(r.rrn);
    }
    assert_eq!(seen, [1, 2, 3, 4, 5]);
    o.setll(&mut t, &KeyArg::Rrn(4)).unwrap();
    assert!(o.fb.found && o.fb.equal);
    assert_eq!(o.read(&mut t, true).unwrap().unwrap().rrn, 4);
    o.delete(&mut t).unwrap();
    o.setgt(&mut t, &KeyArg::Rrn(2)).unwrap();
    let mut rest = Vec::new();
    while let Some(r) = o.read(&mut t, true).unwrap() {
        rest.push(r.rrn);
    }
    assert_eq!(rest, [3, 5]);
    assert_eq!(o.readp(&mut t, true).unwrap().unwrap().rrn, 5);
    assert_eq!(o.readp(&mut t, true).unwrap().unwrap().rrn, 3);
    o.setll(&mut t, &KeyArg::Rrn(4)).unwrap();
    assert!(o.fb.found && !o.fb.equal);
    // Key positioning is refused on an arrival open.
    assert!(matches!(
        o.setll(&mut t, &KeyArg::one(num("1"))),
        Err(DbError::Definition { .. })
    ));
    o.close(&mut t);
    t.commit().unwrap();
}

#[test]
fn locks_level_checks_and_logical_reads() {
    let (_d, mut store, lib) = open_new();
    let file = setup(&mut store, lib);
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    // Two jobs: the second cannot read for update what the first holds.
    let mut a = Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            usage: Usage::UPDATE,
            ..OpenOpts::input(JobHandle(1))
        },
    )
    .unwrap();
    let mut b = Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            usage: Usage::UPDATE,
            ..OpenOpts::input(JobHandle(2))
        },
    )
    .unwrap();
    a.chain(&mut t, &KeyArg::one(num("7")), true).unwrap();
    let e = b.chain(&mut t, &KeyArg::one(num("7")), true).unwrap_err();
    assert!(matches!(
        e,
        DbError::RecordLocked {
            rrn: 4,
            holder: 1,
            ..
        }
    ));
    assert_eq!(b.fb.status_code(), "01218");
    assert_eq!(b.fb.msgid, Some("CPF5027"));
    // Reading without lock is fine; after A unlocks, B may lock.
    assert!(
        b.chain(&mut t, &KeyArg::one(num("7")), false)
            .unwrap()
            .is_some()
    );
    a.unlock_op(&mut t).unwrap();
    assert!(
        b.chain(&mut t, &KeyArg::one(num("7")), true)
            .unwrap()
            .is_some()
    );
    b.close(&mut t);
    a.close(&mut t);
    assert!(locks.lock().unwrap().is_empty());
    // A closed path refuses operations with 01211.
    // (close consumes the path; reopen checks continue below.)
    // Level check.
    let desc = file::describe(&mut t, file).unwrap();
    let good = vec![(name("CUSREC"), desc.formats[0].level_id.clone())];
    Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            lvlchk: Some(good),
            ..OpenOpts::input(JobHandle(3))
        },
    )
    .unwrap()
    .close(&mut t);
    let bad = vec![(name("CUSREC"), "0000000000000".to_string())];
    let e = Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            lvlchk: Some(bad.clone()),
            ..OpenOpts::input(JobHandle(3))
        },
    )
    .unwrap_err();
    assert!(matches!(e, DbError::LevelCheck { .. }));
    assert_eq!(e.msgid(), Some("CPF4131"));
    // LVLCHK(*NO) on the file skips it.
    let mut d2 = desc.clone();
    d2.attr.lvlchk = false;
    store::obj::ops::write_attr(&mut t, d2.page, 0, &d2.attr.encode()).unwrap();
    let o = Odp::open(
        &mut t,
        locks.clone(),
        file,
        OpenOpts {
            lvlchk: Some(bad),
            ..OpenOpts::input(JobHandle(3))
        },
    )
    .unwrap();
    o.close(&mut t);
    // Exclusive file lock by another job blocks the open with 01222.
    t.store()
        .locks_mut()
        .lock(JobHandle(9), file, store::obj::LockState::Exclusive)
        .unwrap();
    let e = Odp::open(&mut t, locks.clone(), file, OpenOpts::input(JobHandle(4))).unwrap_err();
    assert!(matches!(e, DbError::FileLocked { holder: 9, .. }));
    t.store().locks_mut().release_job(JobHandle(9));
    // A logical file read through the ODP: by name, positive balances, derived fields.
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let lfdef = lf(
        &[
            rec("BYNAME", "PFILE(QSTEST/CUST)"),
            fld("NAME", "", "", "", ""),
            fld("CUSNO", "", "", "", ""),
            fld("INIT", "", "", "", "SST(NAME 1 1)"),
            key("NAME", ""),
            sel("CUSNO", "COMP(GT 1)"),
        ],
        &refs,
    );
    let lfid = file::create_lf(
        &mut t,
        lib,
        name("BYNAME"),
        &lfdef,
        &mut |_| Ok(file),
        &CreateFileOpts {
            now: 2,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut l = Odp::open(
        &mut t,
        locks.clone(),
        lfid,
        OpenOpts {
            usage: Usage::UPDATE,
            ..OpenOpts::input(JobHandle(5))
        },
    )
    .unwrap();
    let mut names = Vec::new();
    while let Some(r) = l.read(&mut t, true).unwrap() {
        let init = l.field(&r, "INIT").unwrap();
        assert_eq!(
            init,
            Value::Char(ccsid::to_ebcdic(&who(&l, &r)[..1]).unwrap())
        );
        names.push((who(&l, &r), r.rrn));
    }
    assert_eq!(
        names,
        [
            ("Bob".into(), 3),
            ("Carol".into(), 2),
            ("Grace".into(), 4),
            ("Judy".into(), 5)
        ]
    );
    // The lock through the LF is on the physical record.
    let r = l
        .chain(&mut t, &KeyArg::one(chars("Carol")), true)
        .unwrap()
        .unwrap();
    assert_eq!(r.rrn, 2);
    assert_eq!(
        locks.lock().unwrap().holder(desc.members[0].object, 2),
        Some(JobHandle(5))
    );
    // Writes through the LF are not available yet.
    assert!(l.write(&mut t, &[0; 35], &[]).is_err());
    assert_eq!(l.fb.status_code(), "01299");
    l.close(&mut t);
    assert!(locks.lock().unwrap().is_empty());
    t.commit().unwrap();
}
