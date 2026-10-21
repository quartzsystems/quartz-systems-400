// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access paths: keyed physical members, UNIQUE, maintenance of logical
//! members over them, select/omit, derived fields, multi-format paths,
//! DYNSLT, MAINT states and rebuilds, and a clean RCLSTG throughout.

#![allow(clippy::too_many_lines)]

mod common;

use std::ops::Bound;

use common::{
    customer_def, file_kw, fld, key, lf, name, omit, open_new, pf, rec, record, record_with,
    reopen, sel,
};
use db::file::{self, CreateFileOpts, MemberOpt};
use db::{DbError, Maint, Member, Size};
use dds::MapRefs;
use qsname::MemberSpec;
use store::ReclaimMode;
use store::obj::{JobHandle, ObjectId};

fn rrns(keys: &[db::PathKey]) -> Vec<u64> {
    keys.iter().map(|k| k.rrn).collect()
}

fn names(m: &Member, txn: &mut store::Txn<'_>, keys: &[db::PathKey]) -> Vec<String> {
    keys.iter()
        .map(|k| {
            let s = m.read_entry(txn, k).unwrap().unwrap();
            let f = m
                .format_at(usize::from(k.format))
                .format
                .field(&name("NAME"))
                .unwrap()
                .1;
            ccsid::decode_trimmed(db::record::field_bytes(f, &s.record))
        })
        .collect()
}

#[test]
fn keyed_physical_member() {
    let (d, mut store, lib) = open_new();
    let def = customer_def();
    let (file, mbr) = {
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
        let desc = file::describe(&mut t, id).unwrap();
        t.commit().unwrap();
        (id, desc.members[0].object)
    };
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    assert_eq!(m.id(), mbr);
    assert_eq!(m.attr().key_len, 8);
    assert!(m.path().is_some());
    for (i, c) in [50, -3, 7, 1_000_000, 0].iter().enumerate() {
        let rrn = m
            .write_new(&mut t, &record(*c, &format!("C{i}")), &[0])
            .unwrap();
        assert_eq!(rrn, u64::try_from(i + 1).unwrap());
    }
    assert_eq!(m.attr().path_entries, 5);
    let all = m
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(rrns(&all), [2, 5, 3, 1, 4]);
    let back = m.keys_back(&mut t, Bound::Unbounded, 2).unwrap();
    assert_eq!(rrns(&back), [4, 1]);
    // Point and range lookups by encoded key.
    let k7 = m.key_of(&record(7, "x"), &[0]).unwrap();
    assert_eq!(
        rrns(
            &m.keys(&mut t, Bound::Included(&k7), Bound::Included(&k7), 10)
                .unwrap()
        ),
        [3]
    );
    assert_eq!(
        rrns(
            &m.keys(&mut t, Bound::Excluded(&k7), Bound::Unbounded, 10)
                .unwrap()
        ),
        [1, 4]
    );
    assert_eq!(
        rrns(&m.keys_back(&mut t, Bound::Excluded(&k7), 10).unwrap()),
        [5, 2]
    );
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    // UNIQUE.
    assert!(matches!(
        m.write_new(&mut t, &record(7, "dup"), &[0]),
        Err(DbError::DuplicateKey { .. })
    ));
    assert!(matches!(
        m.update(&mut t, 1, &record(7, "dup"), &[0]),
        Err(DbError::DuplicateKey { .. })
    ));
    // The failed update must not have torn the path; a clean update moves the key.
    t.abort();
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    m.update(&mut t, 1, &record(60, "C0 moved"), &[0]).unwrap();
    assert_eq!(
        rrns(
            &m.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
                .unwrap()
        ),
        [2, 5, 3, 1, 4]
    );
    m.update(&mut t, 4, &record(-10, "C3 moved"), &[0]).unwrap();
    assert_eq!(
        rrns(
            &m.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
                .unwrap()
        ),
        [4, 2, 5, 3, 1]
    );
    m.delete(&mut t, 2).unwrap();
    assert_eq!(
        rrns(
            &m.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
                .unwrap()
        ),
        [4, 5, 3, 1]
    );
    assert_eq!(m.attr().path_entries, 4);
    // read_entry gives the record.
    let first = &m
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 1)
        .unwrap()[0];
    assert_eq!(
        m.read_entry(&mut t, first).unwrap().unwrap().record,
        record(-10, "C3 moved")
    );
    t.commit().unwrap();
    // A rebuild gives the same path; persistence across a reopen.
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    let before = m
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    let mut seen = Vec::new();
    m.rebuild_path(&mut t, &mut |done, total| seen.push((done, total)))
        .unwrap();
    assert_eq!(seen.last(), Some(&(5, 5)));
    assert_eq!(
        m.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
            .unwrap(),
        before
    );
    assert_eq!(m.attr().path_entries, 4);
    t.commit().unwrap();
    assert_eq!(store.reclaim(ReclaimMode::Check, 9).unwrap().problems(), 0);
}

fn orders_def() -> dds::PfDefinition {
    pf(&[
        rec("ORDREC", ""),
        fld("ORDNO", "7", "P", "0", ""),
        fld("OCUSNO", "7", "P", "0", ""),
        fld("OAMT", "9", "P", "2", ""),
        fld("STATUS", "1", "A", "", ""),
        key("ORDNO", ""),
    ])
}

fn order(ordno: i64, cusno: i64, status: &str) -> Vec<u8> {
    let p7 = qdecimal::Packed::new(7, 0).unwrap();
    let mut r = Vec::new();
    r.extend(
        p7.encode(
            &qdecimal::Decimal::new(i128::from(ordno), 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap(),
    );
    r.extend(
        p7.encode(
            &qdecimal::Decimal::new(i128::from(cusno), 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap(),
    );
    r.extend(
        qdecimal::Packed::new(9, 2)
            .unwrap()
            .encode(
                &qdecimal::Decimal::parse("99.99").unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    r.extend(ccsid::to_ebcdic(status).unwrap());
    r
}

/// A customer and an orders file in QSTEST, with records.
fn two_files(store: &mut store::Store, lib: ObjectId) -> (ObjectId, ObjectId, MapRefs) {
    let cust = customer_def();
    let ords = orders_def();
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &cust);
    refs.insert_pf("QSTEST", "ORDERS", &ords);
    let mut t = store.begin().unwrap();
    let c = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &cust,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let o = file::create_pf(
        &mut t,
        lib,
        name("ORDERS"),
        &ords,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut cm = Member::open_by(&mut t, c, MemberSpec::First).unwrap();
    for (n, who, bal) in [
        (3, "Carol", "5.00"),
        (1, "Alice", "250.00"),
        (2, "Bob", "0.00"),
        (4, "Dave", "-20.00"),
    ] {
        cm.write_new(&mut t, &record_with(n, who, bal), &[0])
            .unwrap();
    }
    let mut om = Member::open_by(&mut t, o, MemberSpec::First).unwrap();
    for (ordno, cusno, st) in [(100, 1, "A"), (101, 3, "X"), (102, 1, "A")] {
        om.write_new(&mut t, &order(ordno, cusno, st), &[]).unwrap();
    }
    t.commit().unwrap();
    (c, o, refs)
}

fn resolver(cust: ObjectId, orders: ObjectId) -> impl FnMut(&dds::FileRef) -> db::Result<ObjectId> {
    move |fr: &dds::FileRef| {
        Ok(match fr.file.to_string().as_str() {
            "CUST" => cust,
            "ORDERS" => orders,
            other => panic!("unknown file {other}"),
        })
    }
}

#[test]
fn logical_member_over_a_physical_one() {
    let (d, mut store, lib) = open_new();
    let (cust, orders, refs) = two_files(&mut store, lib);
    // By name, with derived fields, selecting positive balances.
    let def = lf(
        &[
            rec("CUSBYNAME", "PFILE(QSTEST/CUST)"),
            fld("NAME", "", "", "", ""),
            fld("CUSNO", "", "", "", ""),
            fld("INIT", "", "", "", "SST(NAME 1 1)"),
            fld("TAG", "", "", "", "CONCAT(NAME NOTE)"),
            fld("BAL", "", "", "", ""),
            key("NAME", ""),
            sel("BAL", "COMP(GT 0)"),
        ],
        &refs,
    );
    let lfid = {
        let mut t = store.begin().unwrap();
        let id = file::create_lf(
            &mut t,
            lib,
            name("CUSBYNAME"),
            &def,
            &mut resolver(cust, orders),
            &CreateFileOpts {
                now: 2,
                ..CreateFileOpts::default()
            },
        )
        .unwrap();
        t.commit().unwrap();
        id
    };
    let mut t = store.begin().unwrap();
    let desc = file::describe(&mut t, lfid).unwrap();
    assert_eq!(desc.attr.kind, db::FileKind::Logical);
    let log = desc.formats[0].logical.as_ref().unwrap();
    assert_eq!(log.based_on[0].0, cust);
    assert_eq!(log.sources.len(), 5);
    assert_eq!(log.select_omit.len(), 1);
    let lm = Member::open_by(&mut t, lfid, MemberSpec::First).unwrap();
    assert!(lm.is_logical());
    assert_eq!(lm.bases.len(), 1);
    assert_eq!(lm.attr().key_len, 30);
    // Carol 5.00 and Alice 250.00 are selected; Bob 0 and Dave -20 are not.
    let keys = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(names(&lm, &mut t, &keys), ["Alice", "Carol"]);
    let alice = lm.read_entry(&mut t, &keys[0]).unwrap().unwrap();
    let f = &desc.formats[0].format;
    let init = f.field(&name("INIT")).unwrap().1;
    assert_eq!(
        db::record::field_bytes(init, &alice.record),
        ccsid::to_ebcdic("A").unwrap()
    );
    let tag = f.field(&name("TAG")).unwrap().1;
    assert_eq!(tag.length, 130);
    assert_eq!(
        ccsid::decode_trimmed(db::record::field_bytes(tag, &alice.record)),
        format!("{:<30}hello", "Alice")
    );
    // The physical member knows its dependent; writes through it maintain the LF.
    let mut cm = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!(cm.dependents.len(), 1);
    cm.write_new(&mut t, &record_with(5, "Aaron", "1.00"), &[0])
        .unwrap();
    cm.write_new(&mut t, &record_with(6, "Zed", "-1.00"), &[0])
        .unwrap();
    cm.update(&mut t, 3, &record_with(2, "Bob", "10.00"), &[0])
        .unwrap();
    cm.delete(&mut t, 1).unwrap();
    let lm = Member::open_by(&mut t, lfid, MemberSpec::First).unwrap();
    let keys = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(names(&lm, &mut t, &keys), ["Aaron", "Alice", "Bob"]);
    assert_eq!(lm.attr().path_entries, 3);
    // RMVM of the physical member is refused while the LF depends on it.
    assert!(matches!(
        file::remove_member(&mut t, JobHandle(1), cust, name("CUST")),
        Err(DbError::HasDependents { .. })
    ));
    assert!(matches!(
        file::delete_file(&mut t, JobHandle(1), cust),
        Err(DbError::HasDependents { .. })
    ));
    t.commit().unwrap();
    assert_eq!(store.reclaim(ReclaimMode::Check, 9).unwrap().problems(), 0);
    // Reopen: a rebuild reproduces the maintained path.
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let mut lm = Member::open_by(&mut t, lfid, MemberSpec::First).unwrap();
    let before = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    lm.rebuild_path(&mut t, &mut |_, _| {}).unwrap();
    assert_eq!(
        lm.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
            .unwrap(),
        before
    );
    // Deleting the LF frees the PF.
    file::delete_file(&mut t, JobHandle(2), lfid).unwrap();
    let cm = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert!(cm.dependents.is_empty());
    file::delete_file(&mut t, JobHandle(2), cust).unwrap();
    t.commit().unwrap();
    assert_eq!(store.reclaim(ReclaimMode::Check, 10).unwrap().problems(), 0);
}

#[test]
fn multi_format_dynslt_and_maint() {
    let (_d, mut store, lib) = open_new();
    let (cust, orders, refs) = two_files(&mut store, lib);
    // Two formats, keyed on the customer number in each, interleaved.
    let def = lf(
        &[
            rec("CUSREC", "PFILE(QSTEST/CUST)"),
            fld("CUSNO", "", "", "", ""),
            fld("NAME", "", "", "", ""),
            key("CUSNO", ""),
            rec("ORDREC", "PFILE(QSTEST/ORDERS)"),
            fld("OCUSNO", "", "", "", ""),
            fld("ORDNO", "", "", "", ""),
            key("OCUSNO", ""),
        ],
        &refs,
    );
    let mut t = store.begin().unwrap();
    let multi = file::create_lf(
        &mut t,
        lib,
        name("CUSORD"),
        &def,
        &mut resolver(cust, orders),
        &CreateFileOpts {
            now: 3,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let lm = Member::open_by(&mut t, multi, MemberSpec::First).unwrap();
    assert!(lm.is_multi_format());
    assert_eq!(lm.bases.len(), 2);
    let keys = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    let tagged: Vec<(u16, u64)> = keys.iter().map(|k| (k.format, k.rrn)).collect();
    // Customer 1 (rrn 2) then its orders 100 (rrn 1) and 102 (rrn 3), customer 2, 3 + order 101, 4.
    assert_eq!(
        tagged,
        [(0, 2), (1, 1), (1, 3), (0, 3), (0, 1), (1, 2), (0, 4)]
    );
    assert_eq!(lm.attr().path_entries, 7);
    let one = lm.read_entry(&mut t, &keys[1]).unwrap().unwrap();
    assert_eq!(one.record.len(), 8);
    // A new order appears in the interleaved path.
    let mut om = Member::open_by(&mut t, orders, MemberSpec::First).unwrap();
    om.write_new(&mut t, &order(103, 2, "A"), &[]).unwrap();
    let lm = Member::open_by(&mut t, multi, MemberSpec::First).unwrap();
    let keys = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(keys.len(), 8);
    assert_eq!((keys[4].format, keys[4].rrn), (1, 4));
    // DYNSLT: every record is in the path; the test runs on read.
    let dyn_def = lf(
        &[
            file_kw("DYNSLT"),
            rec("ACTIVE", "PFILE(QSTEST/ORDERS)"),
            fld("ORDNO", "", "", "", ""),
            fld("STATUS", "", "", "", ""),
            key("ORDNO", ""),
            omit("STATUS", "COMP(EQ 'X')"),
        ],
        &refs,
    );
    let dynid = file::create_lf(
        &mut t,
        lib,
        name("ACTIVE"),
        &dyn_def,
        &mut resolver(cust, orders),
        &CreateFileOpts {
            now: 4,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let dm = Member::open_by(&mut t, dynid, MemberSpec::First).unwrap();
    assert!(dm.file.attr.dynslt);
    let keys = dm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(keys.len(), 4);
    let read: Vec<bool> = keys
        .iter()
        .map(|k| dm.read_entry(&mut t, k).unwrap().is_some())
        .collect();
    assert_eq!(read, [true, false, true, true]);
    // The static select/omit twin keeps only three.
    let stat_def = lf(
        &[
            rec("ACTIVE2", "PFILE(QSTEST/ORDERS)"),
            fld("ORDNO", "", "", "", ""),
            fld("STATUS", "", "", "", ""),
            key("ORDNO", ""),
            omit("STATUS", "COMP(EQ 'X')"),
            sel("", "ALL"),
        ],
        &refs,
    );
    let statid = file::create_lf(
        &mut t,
        lib,
        name("ACTIVE2"),
        &stat_def,
        &mut resolver(cust, orders),
        &CreateFileOpts {
            now: 5,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let sm = Member::open_by(&mut t, statid, MemberSpec::First).unwrap();
    assert_eq!(sm.attr().path_entries, 3);
    // MAINT(*REBLD): writes invalidate the LF path; a keyed read refuses; ensure_path rebuilds.
    let rebld_def = lf(
        &[
            rec("BYSTAT", "PFILE(QSTEST/ORDERS)"),
            fld("STATUS", "", "", "", ""),
            fld("ORDNO", "", "", "", ""),
            key("STATUS", ""),
            key("ORDNO", "DESCEND"),
        ],
        &refs,
    );
    let rebld = file::create_lf(
        &mut t,
        lib,
        name("BYSTAT"),
        &rebld_def,
        &mut resolver(cust, orders),
        &CreateFileOpts {
            now: 6,
            maint: Maint::Rebld,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut om = Member::open_by(&mut t, orders, MemberSpec::First).unwrap();
    assert_eq!(om.dependents.len(), 4);
    om.write_new(&mut t, &order(104, 4, "B"), &[]).unwrap();
    let mut rm = Member::open_by(&mut t, rebld, MemberSpec::First).unwrap();
    assert!(rm.attr().path_invalid);
    assert!(matches!(
        rm.keys(&mut t, Bound::Unbounded, Bound::Unbounded, 1),
        Err(DbError::PathInvalid { .. })
    ));
    assert!(rm.ensure_path(&mut t, &mut |_, _| {}).unwrap());
    assert!(!rm.ensure_path(&mut t, &mut |_, _| {}).unwrap());
    let keys = rm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert_eq!(rrns(&keys), [4, 3, 1, 5, 2]);
    // MAINT(*DLY) rebuilds at open.
    let mut desc = file::describe(&mut t, rebld).unwrap();
    desc.attr.maint = Maint::Dly;
    store::obj::ops::write_attr(&mut t, desc.page, 0, &desc.attr.encode()).unwrap();
    om.write_new(&mut t, &order(105, 4, "C"), &[]).unwrap();
    let rm = Member::open_by(&mut t, rebld, MemberSpec::First).unwrap();
    assert!(!rm.attr().path_invalid);
    assert_eq!(rm.attr().path_entries, 6);
    // CLRPFM empties dependent paths too.
    om.clear(&mut t).unwrap();
    let lm = Member::open_by(&mut t, multi, MemberSpec::First).unwrap();
    let keys = lm
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 100)
        .unwrap();
    assert!(keys.iter().all(|k| k.format == 0));
    assert_eq!(keys.len(), 4);
    let dm = Member::open_by(&mut t, dynid, MemberSpec::First).unwrap();
    assert_eq!(dm.attr().path_entries, 0);
    t.commit().unwrap();
    assert_eq!(store.reclaim(ReclaimMode::Check, 11).unwrap().problems(), 0);
}

#[test]
fn excnull_and_null_keys() {
    let (_d, mut store, lib) = open_new();
    let def = pf(&[
        file_kw("UNIQUE(*EXCNULL)"),
        rec("R", ""),
        fld("CODE", "3", "A", "", "ALWNULL"),
        fld("NUM", "5", "S", "0", ""),
        key("CODE", ""),
    ]);
    let mut t = store.begin().unwrap();
    let id = file::create_pf(
        &mut t,
        lib,
        name("NULLS"),
        &def,
        &CreateFileOpts {
            now: 1,
            size: Size::NOMAX,
            member: MemberOpt::File,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut m = Member::open_by(&mut t, id, MemberSpec::First).unwrap();
    let rec = |code: &str, n: &str| {
        let mut r = vec![ccsid::SPACE; 3];
        ccsid::encode_fixed(code, &mut r).unwrap();
        r.extend(
            qdecimal::Zoned::new(5, 0)
                .unwrap()
                .encode(
                    &qdecimal::Decimal::parse(n).unwrap(),
                    qdecimal::Rounding::Truncate,
                )
                .unwrap(),
        );
        r
    };
    m.write_new(&mut t, &rec("ABC", "1"), &[0]).unwrap();
    m.write_new(&mut t, &rec("", "2"), &[1]).unwrap();
    m.write_new(&mut t, &rec("", "3"), &[1]).unwrap();
    assert!(matches!(
        m.write_new(&mut t, &rec("ABC", "4"), &[0]),
        Err(DbError::DuplicateKey { .. })
    ));
    let keys = m
        .keys(&mut t, Bound::Unbounded, Bound::Unbounded, 10)
        .unwrap();
    assert_eq!(rrns(&keys), [1, 2, 3]);
    assert_eq!(keys[1].key[0], 0xFF);
    t.commit().unwrap();
}
