// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Overrides and sessions: `OVRDBF` redirection, member selection and
//! positioning, shared open data paths, `INHWRT`, `LVLCHK(*NO)`, member
//! allocation and the job's end.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{fld, key, name, open_new, pf, rec, record};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts, Override, Position, Session, Usage, session::resolve_in};
use db::{DbError, KeyArg, Value, Wait};
use dds::FileRef;
use qdecimal::Decimal;
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};

fn num(n: &str) -> Value {
    Value::Num(Decimal::parse(n).unwrap())
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

fn fileref(s: &str) -> FileRef {
    FileRef::parse(s).unwrap()
}

/// CUST (members CUST and ARCH) and CUST2 with their customers.
fn setup(store: &mut store::Store, lib: ObjectId) -> (ObjectId, ObjectId) {
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
    let opts = CreateFileOpts {
        now: 1,
        max_members: 0,
        ..CreateFileOpts::default()
    };
    let cust = file::create_pf(&mut t, lib, name("CUST"), &def, &opts).unwrap();
    let cust2 = file::create_pf(&mut t, lib, name("CUST2"), &def, &opts).unwrap();
    file::add_member(&mut t, cust, name("ARCH"), "Archive", None, 1).unwrap();
    let fill = |t: &mut store::Txn<'_>, file, mbr, rows: &[(i64, &str)]| {
        let mut m = db::Member::open_by(t, file, mbr).unwrap();
        for (n, w) in rows {
            m.write_new(t, &record(*n, w), &[0]).unwrap();
        }
    };
    fill(
        &mut t,
        cust,
        MemberSpec::First,
        &[
            (1, "Alice"),
            (3, "Carol"),
            (3, "Bob"),
            (7, "Grace"),
            (10, "Judy"),
        ],
    );
    fill(
        &mut t,
        cust,
        MemberSpec::Name(name("ARCH")),
        &[(50, "Olivia"), (60, "Peggy")],
    );
    fill(&mut t, cust2, MemberSpec::First, &[(100, "Victor")]);
    t.commit().unwrap();
    (cust, cust2)
}

#[test]
fn tofile_member_and_position() {
    let (_d, mut store, lib) = open_new();
    let (cust, cust2) = setup(&mut store, lib);
    let mut resolve = resolve_in(
        name("QSTEST"),
        vec![(name("CUST"), cust), (name("CUST2"), cust2)],
    );
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks.clone());
    let mut t = store.begin().unwrap();
    let opts = OpenOpts::input(JobHandle(0));
    // No override: the program's file.
    let o = s
        .open(&mut t, &fileref("CUST"), &mut resolve, opts.clone())
        .unwrap();
    {
        let mut o = o.borrow_mut();
        let r = o.read(&mut t, false).unwrap().unwrap();
        assert_eq!(who(&o, &r), "Alice");
        assert_eq!(o.opts().job, JobHandle(1));
        assert_eq!(o.opts().wait_rcd, Some(Wait::Seconds(60)));
        assert!(!o.opts().share);
    }
    s.close(&mut t, &o);
    assert!(!o.borrow().is_open());
    // TOFILE redirects to CUST2.
    s.ovrdbf(Override::new(name("CUST")).tofile(fileref("QSTEST/CUST2")));
    let o = s
        .open(&mut t, &fileref("CUST"), &mut resolve, opts.clone())
        .unwrap();
    {
        let mut o = o.borrow_mut();
        let r = o.read(&mut t, false).unwrap().unwrap();
        assert_eq!((cusno(&o, &r), who(&o, &r)), (100, "Victor".into()));
        assert_eq!(o.file, cust2);
    }
    s.close(&mut t, &o);
    // A second OVRDBF of the same file replaces the first; MBR picks ARCH.
    s.ovrdbf(Override::new(name("CUST")).member(MemberSpec::Name(name("ARCH"))));
    assert_eq!(s.overrides().len(), 1);
    let o = s
        .open(&mut t, &fileref("CUST"), &mut resolve, opts.clone())
        .unwrap();
    {
        let mut o = o.borrow_mut();
        let r = o.read(&mut t, false).unwrap().unwrap();
        assert_eq!(who(&o, &r), "Olivia");
        assert_eq!(o.member().name(), name("ARCH"));
    }
    s.close(&mut t, &o);
    // DLTOVR of a file not overridden removes nothing.
    assert_eq!(s.dltovr(Some(name("OTHER"))), 0);
    assert_eq!(s.dltovr(Some(name("CUST"))), 1);
    assert!(s.overrides().is_empty());
    // POSITION: the first read after the open.
    let mut first = |s: &mut Session, t: &mut store::Txn<'_>, p: Position| {
        s.ovrdbf(Override::new(name("CUST")).position(p));
        let o = s
            .open(t, &fileref("CUST"), &mut resolve, opts.clone())
            .unwrap();
        let got = {
            let mut o = o.borrow_mut();
            assert_eq!(o.fb.status, odp::status::OK);
            assert!(!o.fb.eof);
            o.read(t, false).unwrap().map(|r| who(&o, &r))
        };
        s.close(t, &o);
        got
    };
    assert_eq!(first(&mut s, &mut t, Position::Start).unwrap(), "Alice");
    assert_eq!(first(&mut s, &mut t, Position::End), None);
    assert_eq!(first(&mut s, &mut t, Position::Rrn(4)).unwrap(), "Grace");
    let k = |n: &str| vec![num(n)];
    assert_eq!(
        first(&mut s, &mut t, Position::KeyAfter(k("3"))).unwrap(),
        "Grace"
    );
    assert_eq!(
        first(&mut s, &mut t, Position::KeyAfterEqual(k("3"))).unwrap(),
        "Bob"
    );
    assert_eq!(
        first(&mut s, &mut t, Position::KeyAfterEqual(k("5"))).unwrap(),
        "Grace"
    );
    assert_eq!(
        first(&mut s, &mut t, Position::KeyBefore(k("7"))).unwrap(),
        "Carol"
    );
    assert_eq!(
        first(&mut s, &mut t, Position::KeyBeforeEqual(k("7"))).unwrap(),
        "Grace"
    );
    assert_eq!(
        first(&mut s, &mut t, Position::KeyBeforeEqual(k("5"))).unwrap(),
        "Carol"
    );
    // Before the first record there is nothing: the first record it is.
    assert_eq!(
        first(&mut s, &mut t, Position::KeyBefore(k("1"))).unwrap(),
        "Alice"
    );
    // A key position needs a keyed open.
    s.ovrdbf(Override::new(name("CUST")).position(Position::KeyAfter(k("3"))));
    let e = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts {
                keyed: false,
                ..opts.clone()
            },
        )
        .unwrap_err();
    assert!(matches!(e, DbError::Definition { .. }));
    assert!(t.store().locks().holders(cust).is_empty());
    s.dltovr(None);
    // An override of a file the program does not open changes nothing.
    s.ovrdbf(Override::new(name("CUST2")).member(MemberSpec::Name(name("ARCH"))));
    let o = s
        .open(&mut t, &fileref("CUST"), &mut resolve, opts.clone())
        .unwrap();
    assert_eq!(o.borrow().member().name(), name("CUST"));
    s.close(&mut t, &o);
    // A missing file through TOFILE is the store's not-found error.
    s.ovrdbf(Override::new(name("CUST")).tofile(fileref("NOPE")));
    let e = s
        .open(&mut t, &fileref("CUST"), &mut resolve, opts)
        .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF9801"));
    s.end(&mut t);
    assert!(s.overrides().is_empty());
    assert!(locks.lock().unwrap().is_empty());
}

#[test]
fn shared_open_data_paths() {
    let (_d, mut store, lib) = open_new();
    let (cust, cust2) = setup(&mut store, lib);
    let mut resolve = resolve_in(
        name("QSTEST"),
        vec![(name("CUST"), cust), (name("CUST2"), cust2)],
    );
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks.clone());
    let mut t = store.begin().unwrap();
    s.ovrdbf(Override::new(name("CUST")).share(true));
    let update = OpenOpts {
        usage: Usage::UPDATE,
        ..OpenOpts::input(JobHandle(0))
    };
    let a = s
        .open(&mut t, &fileref("CUST"), &mut resolve, update.clone())
        .unwrap();
    assert!(a.borrow().opts().share);
    // The second open of the same file and member is the same path, and
    // it carries the first one's position.
    let b = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert!(std::rc::Rc::ptr_eq(&a, &b));
    assert_eq!(s.shared_count(), 1);
    assert_eq!(s.shared_uses(cust, name("CUST")), 2);
    {
        let mut a = a.borrow_mut();
        let r = a.read(&mut t, false).unwrap().unwrap();
        assert_eq!(who(&a, &r), "Alice");
    }
    {
        let mut b = b.borrow_mut();
        let r = b.read(&mut t, false).unwrap().unwrap();
        assert_eq!(who(&b, &r), "Bob");
    }
    // Asking for more than the shared path allows is refused.
    let e = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts {
                usage: Usage::ALL,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .unwrap_err();
    assert!(matches!(e, DbError::ShareMismatch { .. }));
    assert_eq!(e.msgid(), Some("CPF4123"));
    // Another member is another path.
    s.ovrdbf(
        Override::new(name("ARCH"))
            .tofile(fileref("CUST"))
            .member(MemberSpec::Name(name("ARCH")))
            .share(true),
    );
    let c = s
        .open(
            &mut t,
            &fileref("ARCH"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert!(!std::rc::Rc::ptr_eq(&a, &c));
    assert_eq!(s.shared_count(), 2);
    // The first close keeps the path open; the last releases the file.
    s.close(&mut t, &a);
    assert!(b.borrow().is_open());
    assert_eq!(s.shared_uses(cust, name("CUST")), 1);
    assert_eq!(t.store().locks().holders(cust).len(), 2);
    s.close(&mut t, &b);
    assert!(!b.borrow().is_open());
    assert_eq!(s.shared_count(), 1);
    assert_eq!(t.store().locks().holders(cust).len(), 1);
    // CLOF closes the file's shared paths whoever uses them.
    let c2 = s
        .open(
            &mut t,
            &fileref("ARCH"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert!(std::rc::Rc::ptr_eq(&c, &c2));
    assert_eq!(s.clof(&mut t, name("CUST")), 1);
    assert_eq!(s.clof(&mut t, name("CUST")), 0);
    assert!(!c.borrow().is_open());
    assert!(t.store().locks().holders(cust).is_empty());
    // A closed path answers 01211.
    let e = c.borrow_mut().read(&mut t, false).unwrap_err();
    assert!(matches!(e, DbError::Closed));
    assert_eq!(c.borrow().fb.status_code(), "01211");
    // SHARE(*NO) on the override beats SHARE(*YES) on the file.
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    let shared = file::create_pf(
        &mut t,
        lib,
        name("SHR"),
        &common::customer_def(),
        &CreateFileOpts {
            now: 1,
            share: true,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut resolve = resolve_in(name("QSTEST"), vec![(name("SHR"), shared)]);
    let x = s
        .open(
            &mut t,
            &fileref("SHR"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert!(x.borrow().opts().share);
    assert_eq!(s.shared_count(), 1);
    s.ovrdbf(Override::new(name("SHR")).share(false));
    let y = s
        .open(
            &mut t,
            &fileref("SHR"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert!(!std::rc::Rc::ptr_eq(&x, &y));
    assert!(!y.borrow().opts().share);
    s.close(&mut t, &y);
    s.end(&mut t);
    assert!(!x.borrow().is_open());
    assert_eq!(s.shared_count(), 0);
    assert!(t.store().locks().holders(shared).is_empty());
}

#[test]
fn inhwrt_lvlchk_and_member_allocation() {
    let (_d, mut store, lib) = open_new();
    let (cust, cust2) = setup(&mut store, lib);
    let mut resolve = resolve_in(
        name("QSTEST"),
        vec![(name("CUST"), cust), (name("CUST2"), cust2)],
    );
    let locks = odp::new_shared();
    let mut s = Session::new(JobHandle(1), locks.clone());
    let mut t = store.begin().unwrap();
    // INHWRT: writes, updates and deletes report success and change nothing.
    s.ovrdbf(Override::new(name("CUST")).inhwrt(true));
    let o = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts {
                usage: Usage::ALL,
                ..OpenOpts::input(JobHandle(0))
            },
        )
        .unwrap();
    {
        let mut o = o.borrow_mut();
        assert_eq!(o.write(&mut t, &record(99, "Zed"), &[0]).unwrap(), 0);
        let r = o
            .chain(&mut t, &KeyArg::one(num("7")), true)
            .unwrap()
            .unwrap();
        o.update(&mut t, &record(7, "Gone"), &r.null_map).unwrap();
        assert_eq!(o.fb.rrn, 4);
        o.chain(&mut t, &KeyArg::one(num("7")), true).unwrap();
        o.delete(&mut t).unwrap();
        // The record lock went with the update.
        assert!(locks.lock().unwrap().is_empty());
        let r = o
            .chain(&mut t, &KeyArg::one(num("7")), false)
            .unwrap()
            .unwrap();
        assert_eq!(who(&o, &r), "Grace");
        assert!(
            o.chain(&mut t, &KeyArg::one(num("99")), false)
                .unwrap()
                .is_none()
        );
        assert_eq!(o.member().attr().record_count, 5);
    }
    s.close(&mut t, &o);
    s.dltovr(None);
    // LVLCHK(*NO) lets a wrong level identifier through.
    let bad = vec![(name("CUSREC"), "0000000000000".to_string())];
    let with_bad = OpenOpts {
        lvlchk: Some(bad),
        ..OpenOpts::input(JobHandle(0))
    };
    let e = s
        .open(&mut t, &fileref("CUST"), &mut resolve, with_bad.clone())
        .unwrap_err();
    assert!(matches!(e, DbError::LevelCheck { .. }));
    s.ovrdbf(Override::new(name("CUST")).lvlchk(false));
    let o = s
        .open(&mut t, &fileref("CUST"), &mut resolve, with_bad)
        .unwrap();
    assert!(o.borrow().opts().lvlchk.is_none());
    // WAITRCD and SEQONLY reach the open options.
    s.ovrdbf(
        Override::new(name("CUST"))
            .wait_rcd(Wait::NoMax)
            .seqonly(100)
            .nbrrcds(10)
            .secure(),
    );
    let w = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap();
    assert_eq!(w.borrow().opts().wait_rcd, Some(Wait::NoMax));
    assert_eq!(w.borrow().opts().seqonly, Some(100));
    assert!(s.override_for(name("CUST")).unwrap().secure);
    s.close(&mut t, &w);
    // An open path holds the member: an exclusive allocation by another
    // job fails until the path closes.
    let member = o.borrow().member().id();
    let e = odp::allocate(&mut t, JobHandle(2), member, LockState::Exclusive, "CUST").unwrap_err();
    assert!(matches!(e, DbError::FileLocked { holder: 1, .. }));
    assert_eq!(e.msgid(), Some("CPF4128"));
    // The other member of the file is free.
    let desc = file::describe(&mut t, cust).unwrap();
    let arch = desc.member(MemberSpec::Name(name("ARCH"))).unwrap().object;
    odp::allocate(&mut t, JobHandle(2), arch, LockState::Exclusive, "CUST").unwrap();
    odp::deallocate(&mut t, JobHandle(2), arch, LockState::Exclusive);
    s.close(&mut t, &o);
    odp::allocate(&mut t, JobHandle(2), member, LockState::Exclusive, "CUST").unwrap();
    // And an open against an exclusively held member fails.
    let e = s
        .open(
            &mut t,
            &fileref("CUST"),
            &mut resolve,
            OpenOpts::input(JobHandle(0)),
        )
        .unwrap_err();
    assert!(matches!(e, DbError::FileLocked { holder: 2, .. }));
    // The failed open left no file lock behind.
    assert_eq!(t.store().locks().holders(cust).len(), 0);
    odp::deallocate(&mut t, JobHandle(2), member, LockState::Exclusive);
    s.end(&mut t);
}

#[test]
fn call_levels_merge_and_end() {
    let mut s = Session::new(JobHandle(1), odp::new_shared());
    // Command level redirects; a program two levels down picks a member.
    s.ovrdbf(Override::new(name("CUST")).tofile(fileref("QSTEST/CUST2")));
    s.ovrdbf(
        Override::new(name("CUST"))
            .member(MemberSpec::Name(name("ARCH")))
            .level(2),
    );
    let o = s.override_for(name("CUST")).unwrap();
    assert_eq!(o.tofile, Some(fileref("QSTEST/CUST2")), "merged from level 0");
    assert_eq!(o.member, Some(MemberSpec::Name(name("ARCH"))));
    assert_eq!(o.level, 2);
    // The deeper level's value wins where both say something.
    s.ovrdbf(Override::new(name("CUST")).tofile(fileref("QSTEST/CUST3")).level(3));
    assert_eq!(
        s.override_for(name("CUST")).unwrap().tofile,
        Some(fileref("QSTEST/CUST3"))
    );
    // SECURE(*YES) at level 3 hides the levels below.
    s.ovrdbf(Override::new(name("CUST")).share(true).secure().level(3));
    let o = s.override_for(name("CUST")).unwrap();
    assert_eq!(o.share, Some(true));
    assert!(o.tofile.is_none() && o.member.is_none());
    // Level 3 returned: its override goes, the merge resumes.
    assert_eq!(s.end_level(3), 1);
    let o = s.override_for(name("CUST")).unwrap();
    assert_eq!(o.tofile, Some(fileref("QSTEST/CUST2")));
    assert_eq!(o.member, Some(MemberSpec::Name(name("ARCH"))));
    // DLTOVR at level 2 leaves command level's alone.
    assert_eq!(s.dltovr_at(None, 2), 1);
    assert_eq!(s.overrides().len(), 1);
    assert_eq!(s.dltovr_at(Some(name("CUST")), 0), 1);
    assert!(s.overrides().is_empty());
}
