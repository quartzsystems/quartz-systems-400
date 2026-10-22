// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Join logical files: one record per matching secondary record,
//! `JDUPSEQ` order, `JDFTVAL` defaults, a keyed secondary, select/omit
//! on a secondary field, and reads in both directions through the open
//! data path.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{at, customer_def, fld, key, lf, name, open_new, pf, rec, record_with, sel};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts};
use db::{Member, Value};
use dds::MapRefs;
use qsname::MemberSpec;
use store::obj::{JobHandle, ObjectId};

fn orders_def() -> dds::PfDefinition {
    pf(&[
        common::file_kw("UNIQUE"),
        rec("ORDREC", ""),
        fld("ORDNO", "7", "P", "0", ""),
        fld("OCUSNO", "7", "P", "0", ""),
        fld("OAMT", "9", "P", "2", ""),
        key("ORDNO", ""),
    ])
}

fn order(ordno: i64, cusno: i64) -> Vec<u8> {
    let p7 = qdecimal::Packed::new(7, 0).unwrap();
    let enc = |n: i64| {
        p7.encode(
            &qdecimal::Decimal::new(i128::from(n), 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap()
    };
    let mut r = enc(ordno);
    r.extend(enc(cusno));
    r.extend(
        qdecimal::Packed::new(9, 2)
            .unwrap()
            .encode(
                &qdecimal::Decimal::parse("9.99").unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    r
}

/// A `J` line and its keyword continuations.
fn join(kw: &str) -> String {
    at(&[(6, "A"), (17, "J"), (45, kw)])
}

fn cont(kw: &str) -> String {
    at(&[(6, "A"), (45, kw)])
}

/// CUST (1 Alice, 2 Bob, 3 Carol, 4 Dave) and ORDERS (100 and 102 for
/// Alice, 101 for Carol).
fn two_files(store: &mut store::Store, lib: ObjectId) -> (ObjectId, ObjectId, MapRefs) {
    let cust = customer_def();
    let ords = orders_def();
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &cust);
    refs.insert_pf("QSTEST", "ORDERS", &ords);
    let mut t = store.begin().unwrap();
    let opts = CreateFileOpts {
        now: 1,
        ..CreateFileOpts::default()
    };
    let c = file::create_pf(&mut t, lib, name("CUST"), &cust, &opts).unwrap();
    let o = file::create_pf(&mut t, lib, name("ORDERS"), &ords, &opts).unwrap();
    let mut cm = Member::open_by(&mut t, c, MemberSpec::First).unwrap();
    for (n, who) in [(1, "Alice"), (2, "Bob"), (3, "Carol"), (4, "Dave")] {
        cm.write_new(&mut t, &record_with(n, who, "1.00"), &[0])
            .unwrap();
    }
    let mut om = Member::open_by(&mut t, o, MemberSpec::First).unwrap();
    for (ordno, cusno) in [(100, 1), (101, 3), (102, 1)] {
        om.write_new(&mut t, &order(ordno, cusno), &[]).unwrap();
    }
    t.commit().unwrap();
    (c, o, refs)
}

fn create(
    store: &mut store::Store,
    lib: ObjectId,
    lfname: &str,
    lines: &[String],
    refs: &MapRefs,
    cust: ObjectId,
    orders: ObjectId,
) -> ObjectId {
    let def = lf(lines, refs);
    let mut t = store.begin().unwrap();
    let mut resolve = |fr: &dds::FileRef| {
        Ok(match fr.file.to_string().as_str() {
            "CUST" => cust,
            "ORDERS" => orders,
            other => panic!("unknown file {other}"),
        })
    };
    let id = file::create_lf(
        &mut t,
        lib,
        name(lfname),
        &def,
        &mut resolve,
        &CreateFileOpts {
            now: 2,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    t.commit().unwrap();
    id
}

fn num(o: &Odp, r: &odp::Record, f: &str) -> i64 {
    match o.field(r, f).unwrap() {
        Value::Num(d) => i64::try_from(d.to_i128().unwrap()).unwrap(),
        v => panic!("{v:?}"),
    }
}

fn text(o: &Odp, r: &odp::Record, f: &str) -> String {
    match o.field(r, f).unwrap() {
        Value::Char(b) => ccsid::decode_trimmed(&b),
        v => panic!("{v:?}"),
    }
}

/// Every record of the file in key order, forward, as (CUSNO, NAME, ORDNO).
fn all(store: &mut store::Store, lfid: ObjectId, forward: bool) -> Vec<(i64, String, i64)> {
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let mut o = Odp::open(&mut t, locks, lfid, OpenOpts::input(JobHandle(1))).unwrap();
    if !forward {
        o.setll(&mut t, &db::KeyArg::End).unwrap();
    }
    let mut out = Vec::new();
    loop {
        let r = if forward {
            o.read(&mut t, false).unwrap()
        } else {
            o.readp(&mut t, false).unwrap()
        };
        let Some(r) = r else { break };
        out.push((
            num(&o, &r, "CUSNO"),
            text(&o, &r, "NAME"),
            num(&o, &r, "ORDNO"),
        ));
    }
    o.close(&mut t);
    t.abort();
    out
}

#[test]
fn one_record_per_matching_secondary() {
    let (_d, mut store, lib) = open_new();
    let (cust, orders, refs) = two_files(&mut store, lib);
    let base = [
        rec("JOINREC", "JFILE(QSTEST/CUST QSTEST/ORDERS)"),
        join("JOIN(1 2)"),
        cont("JFLD(CUSNO OCUSNO)"),
        fld("CUSNO", "", "", "", ""),
        fld("NAME", "", "", "", ""),
        fld("ORDNO", "", "", "", ""),
        key("CUSNO", ""),
    ];
    let plain = create(&mut store, lib, "CUSORDJ", &base, &refs, cust, orders);
    // Alice twice, Carol once; Bob and Dave have no order.
    let fwd = all(&mut store, plain, true);
    assert_eq!(
        fwd,
        [
            (1, "Alice".into(), 100),
            (1, "Alice".into(), 102),
            (3, "Carol".into(), 101)
        ]
    );
    let mut back = all(&mut store, plain, false);
    back.reverse();
    assert_eq!(back, fwd, "READP walks the same records");
    // The path holds one entry per primary record; the secondary expands.
    {
        let mut t = store.begin().unwrap();
        let lm = Member::open_by(&mut t, plain, MemberSpec::First).unwrap();
        assert_eq!(lm.attr().path_entries, 4);
        let keys = lm
            .keys(
                &mut t,
                std::ops::Bound::Unbounded,
                std::ops::Bound::Unbounded,
                10,
            )
            .unwrap();
        assert_eq!(lm.read_entries(&mut t, &keys[0]).unwrap().len(), 2);
        assert!(lm.read_entries(&mut t, &keys[1]).unwrap().is_empty());
        t.abort();
    }
    // JDUPSEQ descending on the order number.
    let mut dup = base.to_vec();
    dup.insert(3, cont("JDUPSEQ(ORDNO *DESCEND)"));
    let dupid = create(&mut store, lib, "CUSORDD", &dup, &refs, cust, orders);
    assert_eq!(
        all(&mut store, dupid, true),
        [
            (1, "Alice".into(), 102),
            (1, "Alice".into(), 100),
            (3, "Carol".into(), 101)
        ]
    );
    // JDFTVAL keeps the customers without orders, the order fields zero.
    let mut dft = vec![common::file_kw("JDFTVAL")];
    dft.extend(base.iter().cloned());
    let dftid = create(&mut store, lib, "CUSORDV", &dft, &refs, cust, orders);
    assert_eq!(
        all(&mut store, dftid, true),
        [
            (1, "Alice".into(), 100),
            (1, "Alice".into(), 102),
            (2, "Bob".into(), 0),
            (3, "Carol".into(), 101),
            (4, "Dave".into(), 0)
        ]
    );
    // A select on a secondary field runs at read time.
    let mut selected = base.to_vec();
    selected.push(sel("ORDNO", "COMP(EQ 102)"));
    let selid = create(&mut store, lib, "CUSORDS", &selected, &refs, cust, orders);
    assert_eq!(all(&mut store, selid, true), [(1, "Alice".into(), 102)]);
    // The other way round: ORDERS primary, CUST found through its unique
    // key on CUSNO.
    let other = [
        rec("ORDCUSR", "JFILE(QSTEST/ORDERS QSTEST/CUST)"),
        join("JOIN(1 2)"),
        cont("JFLD(OCUSNO CUSNO)"),
        fld("ORDNO", "", "", "", ""),
        fld("CUSNO", "", "", "", ""),
        fld("NAME", "", "", "", ""),
        key("ORDNO", ""),
    ];
    let otherid = create(&mut store, lib, "ORDCUSJ", &other, &refs, cust, orders);
    assert_eq!(
        all(&mut store, otherid, true),
        [
            (1, "Alice".into(), 100),
            (3, "Carol".into(), 101),
            (1, "Alice".into(), 102)
        ]
    );
    // A new order joins at once: the join reads the secondary at read time.
    {
        let mut t = store.begin().unwrap();
        let mut om = Member::open_by(&mut t, orders, MemberSpec::First).unwrap();
        om.write_new(&mut t, &order(103, 2), &[]).unwrap();
        t.commit().unwrap();
    }
    assert_eq!(all(&mut store, plain, true).len(), 4);
    assert_eq!(all(&mut store, otherid, true).len(), 4);
}
