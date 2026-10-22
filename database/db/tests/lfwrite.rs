// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writes, updates and deletes through a logical file: the based-on
//! record takes the mapped fields, keeps or defaults the rest, and the
//! logical path follows; a derived field refuses.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{customer_def, fld, key, lf, name, open_new, rec, record_with};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts, Usage};
use db::{DbError, KeyArg, Member, Value};
use dds::MapRefs;
use qdecimal::Decimal;
use qsname::MemberSpec;
use store::obj::{JobHandle, ObjectId};

fn chars(s: &str) -> Value {
    Value::Char(ccsid::to_ebcdic(s).unwrap())
}

fn num(n: &str) -> Value {
    Value::Num(Decimal::parse(n).unwrap())
}

/// CUST with three customers and a logical file over `NAME`, `CUSNO`
/// and `BAL`, keyed on `NAME`; a second one with a derived field.
fn setup(store: &mut store::Store, lib: ObjectId) -> (ObjectId, ObjectId, ObjectId) {
    let cust = customer_def();
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &cust);
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
    let mut cm = Member::open_by(&mut t, c, MemberSpec::First).unwrap();
    for (n, who, bal) in [
        (1, "Alice", "250.00"),
        (2, "Bob", "0.00"),
        (3, "Carol", "5.00"),
    ] {
        cm.write_new(&mut t, &record_with(n, who, bal), &[0])
            .unwrap();
    }
    let plain = lf(
        &[
            rec("CUSBYNAME", "PFILE(QSTEST/CUST)"),
            fld("NAME", "", "", "", ""),
            fld("CUSNO", "", "", "", ""),
            fld("BAL", "", "", "", ""),
            key("NAME", ""),
        ],
        &refs,
    );
    let derived = lf(
        &[
            rec("CUSINIT", "PFILE(QSTEST/CUST)"),
            fld("CUSNO", "", "", "", ""),
            fld("INIT", "", "", "", "SST(NAME 1 1)"),
            key("CUSNO", ""),
        ],
        &refs,
    );
    let mut resolve = |_: &dds::FileRef| Ok(c);
    let opts = CreateFileOpts {
        now: 2,
        ..CreateFileOpts::default()
    };
    let l1 = file::create_lf(&mut t, lib, name("CUSBYNAME"), &plain, &mut resolve, &opts).unwrap();
    let l2 = file::create_lf(&mut t, lib, name("CUSINIT"), &derived, &mut resolve, &opts).unwrap();
    t.commit().unwrap();
    (c, l1, l2)
}

/// A record of the logical format from field values.
fn lf_record(o: &Odp, values: &[(&str, Value)]) -> Vec<u8> {
    let fmt = &o.member().format().format;
    let mut rec = vec![0u8; usize::try_from(fmt.record_len).unwrap()];
    for (n, v) in values {
        let (_, f) = fmt.field(&name(n)).unwrap();
        db::record::write_field(f, &mut rec, v).unwrap();
    }
    rec
}

#[test]
fn write_update_delete_through_a_logical_file() {
    let (_d, mut store, lib) = open_new();
    let (cust, lfid, derived) = setup(&mut store, lib);
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let opts = OpenOpts {
        usage: Usage::ALL,
        ..OpenOpts::input(JobHandle(1))
    };
    let mut o = Odp::open(&mut t, locks.clone(), lfid, opts.clone()).unwrap();
    // WRITE: the based-on record gets NAME, CUSNO, BAL; SINCE and NOTE
    // their defaults (blank, not null); the path gains the key.
    let zed = lf_record(
        &o,
        &[
            ("NAME", chars("Zed")),
            ("CUSNO", num("9")),
            ("BAL", num("1.25")),
        ],
    );
    let rrn = o.write(&mut t, &zed, &[]).unwrap();
    assert_eq!(rrn, 4);
    assert_eq!(o.fb.status_code(), "00000");
    let pf = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let s = pf.read(&mut t, 4).unwrap().unwrap();
    let pfmt = &pf.format().format;
    let (_, bal) = pfmt.field(&name("BAL")).unwrap();
    assert_eq!(
        db::record::read_field(bal, &s.record, &s.null_map, None).unwrap(),
        num("1.25")
    );
    let (si, f) = pfmt.field(&name("SINCE")).unwrap();
    let v = db::record::read_field(f, &s.record, &s.null_map, pf.format().null_index(si)).unwrap();
    assert_eq!(v, Value::Char(vec![ccsid::SPACE; 10]));
    assert_eq!(pf.attr().record_count, 4);
    // The logical path sees it, in key order.
    o.setll(&mut t, &KeyArg::one(chars("Zed"))).unwrap();
    assert!(o.fb.equal);
    // UPDATE through the path: BAL changes, NOTE on the based-on record stays.
    let bob = o
        .chain(&mut t, &KeyArg::one(chars("Bob")), true)
        .unwrap()
        .unwrap();
    assert_eq!(o.field(&bob, "CUSNO").unwrap(), num("2"));
    let changed = lf_record(
        &o,
        &[
            ("NAME", chars("Bob")),
            ("CUSNO", num("2")),
            ("BAL", num("77.00")),
        ],
    );
    o.update(&mut t, &changed, &[]).unwrap();
    let s = pf.read(&mut t, 2).unwrap().unwrap();
    assert_eq!(
        db::record::read_field(bal, &s.record, &s.null_map, None).unwrap(),
        num("77.00")
    );
    let (_, note) = pfmt.field(&name("NOTE")).unwrap();
    assert_eq!(
        ccsid::decode_trimmed(db::record::field_bytes(note, &s.record)),
        "hello"
    );
    // A key change through the path moves the entry.
    let renamed = lf_record(
        &o,
        &[
            ("NAME", chars("Carl")),
            ("CUSNO", num("3")),
            ("BAL", num("5.00")),
        ],
    );
    o.chain(&mut t, &KeyArg::one(chars("Carol")), true)
        .unwrap()
        .unwrap();
    o.update(&mut t, &renamed, &[]).unwrap();
    assert!(
        o.chain(&mut t, &KeyArg::one(chars("Carol")), false)
            .unwrap()
            .is_none()
    );
    assert!(
        o.chain(&mut t, &KeyArg::one(chars("Carl")), true)
            .unwrap()
            .is_some()
    );
    // DELETE through the path.
    o.delete(&mut t).unwrap();
    assert!(pf.read(&mut t, 3).unwrap().unwrap().deleted);
    let mut names = Vec::new();
    o.setll(&mut t, &KeyArg::Start).unwrap();
    while let Some(r) = o.read(&mut t, false).unwrap() {
        match o.field(&r, "NAME").unwrap() {
            Value::Char(b) => names.push(ccsid::decode_trimmed(&b)),
            v => panic!("{v:?}"),
        }
    }
    assert_eq!(names, ["Alice", "Bob", "Zed"]);
    o.close(&mut t);
    // A format with a derived field cannot be written through.
    let mut d = Odp::open(&mut t, locks.clone(), derived, opts).unwrap();
    let rec = lf_record(&d, &[("CUSNO", num("50")), ("INIT", chars("Q"))]);
    let e = d.write(&mut t, &rec, &[]).unwrap_err();
    assert!(
        matches!(e, DbError::Definition { ref what } if what.contains("INIT is derived")),
        "{e}"
    );
    d.close(&mut t);
    t.commit().unwrap();
}
