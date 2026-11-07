// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Physical file constraints through the open data path, as RPG and RLA
//! see them: unique and primary keys, checks, referential constraints
//! with their delete rules, check pending, and their removal.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{fld, name, open_new, pf, rec};
use db::cst::check::{Cmp, Expr};
use db::cst::ops::{self, NewCst, Pick, Refusal};
use db::cst::{Kind, Rule};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts, Usage, status};
use db::{DbError, Value};
use qdecimal::Decimal;
use qsname::MemberSpec;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

fn int(n: i64) -> Vec<u8> {
    (i32::try_from(n).unwrap()).to_be_bytes().to_vec()
}

/// PAR (ID integer, AMT packed 7,2) and CHD (CID integer, PID integer
/// null-capable).
fn files(t: &mut Txn<'_>, lib: ObjectId) -> (ObjectId, ObjectId) {
    let p = pf(&[
        rec("PARREC", ""),
        fld("ID", "9", "B", "0", ""),
        fld("AMT", "7", "P", "2", ""),
    ]);
    let c = pf(&[
        rec("CHDREC", ""),
        fld("CID", "9", "B", "0", ""),
        fld("PID", "9", "B", "0", "ALWNULL DFT(0)"),
    ]);
    let opts = CreateFileOpts {
        now: 1,
        ..CreateFileOpts::default()
    };
    let par = file::create_pf(t, lib, name("PAR"), &p, &opts).unwrap();
    let chd = file::create_pf(t, lib, name("CHD"), &c, &opts).unwrap();
    (par, chd)
}

fn par_rec(id: i64, amt: &str) -> Vec<u8> {
    let mut r = int(id);
    r.extend(
        qdecimal::Packed::new(7, 2)
            .unwrap()
            .encode(&Decimal::parse(amt).unwrap(), qdecimal::Rounding::Truncate)
            .unwrap(),
    );
    r
}

fn chd_rec(cid: i64, pid: Option<i64>) -> (Vec<u8>, Vec<u8>) {
    let mut r = int(cid);
    r.extend(int(pid.unwrap_or(0)));
    (r, vec![u8::from(pid.is_none())])
}

fn open(t: &mut Txn<'_>, locks: &odp::SharedLocks, file: ObjectId) -> Odp {
    let opts = OpenOpts {
        usage: Usage::ALL,
        keyed: false,
        ..OpenOpts::input(JobHandle(1))
    };
    Odp::open(t, locks.clone(), file, opts).unwrap()
}

fn rows(t: &mut Txn<'_>, file: ObjectId) -> u64 {
    db::Member::open_by(t, file, MemberSpec::First)
        .unwrap()
        .attr()
        .record_count
}

#[test]
fn keys_checks_and_referential_rules() {
    let (_d, mut store, lib) = open_new();
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let (par, chd) = files(&mut t, lib);
    // PRIMARY KEY (ID), CHECK (AMT >= 0).
    let r = ops::add(
        &mut t,
        par,
        &NewCst {
            name: Some("PAR_PK".into()),
            ..NewCst::new(Kind::PrimaryKey, vec![name("ID")])
        },
        2,
    )
    .unwrap();
    assert_eq!(r.names, ["PAR_PK"]);
    let chk = Expr::Cmp(
        Cmp::Ge,
        Box::new(Expr::Col(name("AMT"))),
        Box::new(Expr::Num(Decimal::zero())),
    );
    let r = ops::add(
        &mut t,
        par,
        &NewCst {
            check: Some(("AMT >= 0 ".into(), chk)),
            ..NewCst::new(Kind::Check, vec![])
        },
        2,
    )
    .unwrap();
    assert_eq!(r.names, ["Q_QSTEST_PAR_00001"]);
    // A second primary key, the same key again: refused.
    let e = ops::add(
        &mut t,
        par,
        &NewCst::new(Kind::PrimaryKey, vec![name("AMT")]),
        2,
    )
    .unwrap_err();
    assert!(
        matches!(
            e,
            DbError::Constraint {
                reason: Refusal::SecondPrimary,
                ..
            }
        ),
        "{e:?}"
    );
    let e = ops::add(&mut t, par, &NewCst::new(Kind::Unique, vec![name("ID")]), 2).unwrap_err();
    assert!(
        matches!(
            e,
            DbError::Constraint {
                reason: Refusal::SameKey,
                ..
            }
        ),
        "{e:?}"
    );
    // FOREIGN KEY (PID) REFERENCES PAR ON DELETE CASCADE.
    let fk = NewCst {
        parent: par,
        delete_rule: Rule::Cascade,
        update_rule: Rule::Restrict,
        ..NewCst::new(Kind::Referential, vec![name("PID")])
    };
    let r = ops::add(&mut t, chd, &fk, 2).unwrap();
    assert_eq!(r.names, ["Q_QSTEST_CHD_PID_00001"]);
    assert_eq!(r.messages, [("CPI32B4", "PAR".to_string())]);
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    let mut p = open(&mut t, &locks, par);
    p.write(&mut t, &par_rec(1, "1.00"), &[]).unwrap();
    p.write(&mut t, &par_rec(2, "2.00"), &[]).unwrap();
    // A repeated primary key: CPF5026, status 01021.
    let e = p.write(&mut t, &par_rec(1, "3.00"), &[]).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF5026"));
    assert_eq!(p.fb.status, status::DUPLICATE_KEY);
    // The check: CPF502F, status 01022.
    let e = p.write(&mut t, &par_rec(3, "-1.00"), &[]).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF502F"));
    assert_eq!(p.fb.status, status::CONSTRAINT);
    let mut c = open(&mut t, &locks, chd);
    let (r1, n1) = chd_rec(10, Some(1));
    c.write(&mut t, &r1, &n1).unwrap();
    let (r2, n2) = chd_rec(11, Some(1));
    c.write(&mut t, &r2, &n2).unwrap();
    let (r3, n3) = chd_rec(12, None);
    c.write(&mut t, &r3, &n3).unwrap();
    // No parent: CPF502D.
    let (r4, n4) = chd_rec(13, Some(9));
    let e = c.write(&mut t, &r4, &n4).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF502D"));
    assert_eq!(c.fb.status, status::CONSTRAINT);
    c.close(&mut t);
    // A parent whose dependents cascade is changed only while journaled:
    // CPF4328 whatever the rows, as the primary refuses.
    let mut q = Odp::open(
        &mut t,
        locks.clone(),
        par,
        OpenOpts {
            usage: Usage::ALL,
            keyed: false,
            ..OpenOpts::input(JobHandle(1))
        },
    )
    .unwrap();
    let got = q.read(&mut t, true).unwrap().unwrap();
    assert_eq!(got.rrn, 1);
    let e = q.update(&mut t, &par_rec(5, "1.00"), &[]).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF4328"));
    // Delete with CASCADE needs both files journaled.
    q.read(&mut t, true).unwrap();
    let mut q2 = Odp::open(
        &mut t,
        locks.clone(),
        par,
        OpenOpts {
            usage: Usage::ALL,
            keyed: false,
            ..OpenOpts::input(JobHandle(1))
        },
    )
    .unwrap();
    q2.read(&mut t, true).unwrap().unwrap();
    let e = q2.delete(&mut t).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF4328"));
    q.close(&mut t);
    q2.close(&mut t);
    p.close(&mut t);
    t.abort();

    let mut t = store.begin().unwrap();
    journal(&mut t, lib, &[par, chd]);
    let mut p = open(&mut t, &locks, par);
    p.write(&mut t, &par_rec(1, "1.00"), &[]).unwrap();
    p.write(&mut t, &par_rec(2, "2.00"), &[]).unwrap();
    let mut c = open(&mut t, &locks, chd);
    for (cid, pid) in [(10, Some(1)), (11, Some(1)), (12, Some(2)), (13, None)] {
        let (r, n) = chd_rec(cid, pid);
        c.write(&mut t, &r, &n).unwrap();
    }
    c.close(&mut t);
    p.close(&mut t);
    // A parent key changed under dependents: CPF503A (RESTRICT).
    let mut u = open(&mut t, &locks, par);
    u.read(&mut t, true).unwrap().unwrap();
    assert_eq!(
        u.update(&mut t, &par_rec(5, "1.00"), &[])
            .unwrap_err()
            .msgid(),
        Some("CPF503A")
    );
    u.close(&mut t);
    let mut q = open(&mut t, &locks, par);
    q.read(&mut t, true).unwrap().unwrap();
    q.delete(&mut t).unwrap();
    q.close(&mut t);
    // ID 1's two dependents went with it.
    assert_eq!(rows(&mut t, chd), 2);
    t.commit().unwrap();

    // Removing the primary key others depend on is refused; with cascade
    // the referential constraint goes too.
    let mut t = store.begin().unwrap();
    let fd = db::dsp::fd::dspfd(&mut t, chd)
        .unwrap()
        .render(db::dsp::fd::FdType::Cst);
    assert!(
        fd.contains("Q_QSTEST_CHD_PID_00001")
            && fd.contains("*CASCADE")
            && fd.contains("QSTEST/PAR"),
        "{fd}"
    );
    let pick = Pick {
        name: None,
        kinds: vec![Kind::PrimaryKey],
    };
    let r = ops::remove(&mut t, par, &pick, false, 3).unwrap();
    assert!(r.removed.is_empty() && r.refused == ["PAR_PK"]);
    let r = ops::remove(&mut t, par, &pick, true, 3).unwrap();
    assert_eq!(r.removed, ["PAR_PK"]);
    assert!(db::cst::list(&mut t, chd).unwrap().is_empty());
    t.commit().unwrap();
}

fn journal(t: &mut Txn<'_>, lib: ObjectId, files: &[ObjectId]) {
    let rcv = jrn::rcv::crtjrnrcv(
        t,
        lib,
        name("RCV1"),
        &jrn::rcv::CrtJrnRcvOpts {
            now: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let j = jrn::journal::crtjrn(
        t,
        lib,
        name("JRN"),
        rcv,
        &jrn::journal::CrtJrnOpts {
            now: 1,
            ..Default::default()
        },
    )
    .unwrap();
    for f in files {
        db::journal::strjrnpf(t, JobHandle(1), *f, j, true, false, 1).unwrap();
    }
}

#[test]
fn set_null_set_default_and_check_pending() {
    let (_d, mut store, lib) = open_new();
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let (par, chd) = files(&mut t, lib);
    ops::add(
        &mut t,
        par,
        &NewCst::new(Kind::PrimaryKey, vec![name("ID")]),
        2,
    )
    .unwrap();
    journal(&mut t, lib, &[par, chd]);
    let mut p = open(&mut t, &locks, par);
    for id in [0, 1, 2] {
        p.write(&mut t, &par_rec(id, "1.00"), &[]).unwrap();
    }
    p.close(&mut t);
    let mut c = open(&mut t, &locks, chd);
    for (cid, pid) in [(10, Some(1)), (11, Some(2)), (12, Some(7))] {
        let (r, n) = chd_rec(cid, pid);
        c.write(&mut t, &r, &n).unwrap();
    }
    c.close(&mut t);
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    // A foreign key the records break: SQL refuses it, ADDPFCST leaves it
    // in check pending.
    let fk = NewCst {
        parent: par,
        delete_rule: Rule::SetNull,
        ..NewCst::new(Kind::Referential, vec![name("PID")])
    };
    let e = ops::add(&mut t, chd, &fk, 2).unwrap_err();
    assert!(
        matches!(
            e,
            DbError::Constraint {
                reason: Refusal::RefData,
                ..
            }
        ),
        "{e:?}"
    );
    t.abort();
    let mut t = store.begin().unwrap();
    let fk = NewCst {
        pending_ok: true,
        ..fk
    };
    let r = ops::add(&mut t, chd, &fk, 2).unwrap();
    assert_eq!(
        r.messages.iter().map(|m| m.0).collect::<Vec<_>>(),
        ["CPD32C5", "CPD32B7"]
    );
    let opts = OpenOpts {
        usage: Usage::ALL,
        keyed: false,
        ..OpenOpts::input(JobHandle(1))
    };
    let e = Odp::open(&mut t, locks.clone(), chd, opts.clone()).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF523B"));
    // The record fixed, the constraint enabled again: no longer pending.
    let mut m = db::Member::open_by(&mut t, chd, MemberSpec::First).unwrap();
    m.delete(&mut t, 3).unwrap();
    let name0 = r.names[0].clone();
    let one = Pick {
        name: Some(name0.clone()),
        kinds: vec![],
    };
    let ch = ops::set_state(&mut t, chd, &one, true, 3).unwrap();
    assert!(ch.pending.is_empty());
    // SET NULL: ID 1's dependent keeps its row with PID null.
    let mut q = open(&mut t, &locks, par);
    q.read(&mut t, true).unwrap();
    q.read(&mut t, true).unwrap();
    q.delete(&mut t).unwrap();
    q.close(&mut t);
    let m = db::Member::open_by(&mut t, chd, MemberSpec::First).unwrap();
    let s = m.read(&mut t, 1).unwrap().unwrap();
    assert!(!s.deleted && s.null_map[0] & 1 != 0);
    // Disabled: a record with no parent goes in.
    ops::set_state(&mut t, chd, &one, false, 3).unwrap();
    let mut c = Odp::open(&mut t, locks.clone(), chd, opts.clone()).unwrap();
    let (r, n) = chd_rec(20, Some(99));
    c.write(&mut t, &r, &n).unwrap();
    c.close(&mut t);
    let ch = ops::set_state(&mut t, chd, &one, true, 3).unwrap();
    assert_eq!(ch.pending, std::slice::from_ref(&name0));
    t.commit().unwrap();

    // SET DEFAULT: the default 0 is a parent.
    let mut t = store.begin().unwrap();
    ops::remove(
        &mut t,
        chd,
        &Pick {
            name: None,
            kinds: vec![],
        },
        false,
        4,
    )
    .unwrap();
    let mut m = db::Member::open_by(&mut t, chd, MemberSpec::First).unwrap();
    m.clear(&mut t).unwrap();
    let mut c = Odp::open(&mut t, locks.clone(), chd, opts.clone()).unwrap();
    let (r, n) = chd_rec(30, Some(2));
    c.write(&mut t, &r, &n).unwrap();
    c.close(&mut t);
    let fk = NewCst {
        parent: par,
        delete_rule: Rule::SetDefault,
        ..NewCst::new(Kind::Referential, vec![name("PID")])
    };
    ops::add(&mut t, chd, &fk, 4).unwrap();
    let mut q = open(&mut t, &locks, par);
    while let Some(r) = q.read(&mut t, true).unwrap() {
        if r.data[..4] == int(2)[..] {
            q.delete(&mut t).unwrap();
        }
    }
    q.close(&mut t);
    let m = db::Member::open_by(&mut t, chd, MemberSpec::First).unwrap();
    let s = m.read(&mut t, 1).unwrap().unwrap();
    assert_eq!(&s.record[4..8], &int(0)[..]);
    t.commit().unwrap();
    let _ = Value::Null;
}

#[test]
fn check_program_round_trips() {
    use db::cst::check::{Arith, Func};
    let e = Expr::And(
        Box::new(Expr::Between(
            Box::new(Expr::Col(name("A"))),
            Box::new(Expr::Neg(Box::new(Expr::Num(Decimal::parse("5").unwrap())))),
            Box::new(Expr::Float(9.5)),
            false,
        )),
        Box::new(Expr::Like(
            Box::new(Expr::Call(Func::Upper, vec![Expr::Col(name("B"))])),
            Box::new(Expr::Str("A%".into())),
            None,
            true,
        )),
    );
    let e = Expr::Or(
        Box::new(e),
        Box::new(Expr::In(
            Box::new(Expr::Arith(
                Arith::Add,
                Box::new(Expr::Col(name("A"))),
                Box::new(Expr::Null),
            )),
            vec![Expr::Num(Decimal::zero())],
            false,
        )),
    );
    assert_eq!(Expr::decode(&e.encode()).unwrap(), e);
    assert_eq!(e.columns(), [name("A"), name("B")]);
}
