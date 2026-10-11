// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files and members on a store: create from DDS, describe, members,
//! record slots, reuse of deleted slots, limits, persistence across a
//! reopen, and a clean RCLSTG with members as owned objects.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::{self, CreateFileOpts, MemberOpt};
use db::{DbError, FileKind, Member, Size};
use dds::{CompileOpts, NoRefs, PfDefinition, Source, compile_pf};
use qsname::{MemberSpec, Name};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, ReclaimMode, Store, StoreConfig};

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn open_new() -> (Arc<MemBlockDev>, Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
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
    (d, store, lib)
}

fn reopen(d: &Arc<MemBlockDev>) -> Store {
    Store::open(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0
}

fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

fn fld(name: &str, len: &str, ty: &str, dec: &str, kw: &str) -> String {
    at(&[
        (6, "A"),
        (19, name),
        (35 - len.len(), len),
        (35, ty),
        (38 - dec.len(), dec),
        (45, kw),
    ])
}

/// A customer file: packed key, text, a null-capable date, a varlen note.
fn customer_def() -> PfDefinition {
    let src = [
        at(&[(6, "A"), (45, "UNIQUE")]),
        at(&[
            (6, "A"),
            (17, "R"),
            (19, "CUSREC"),
            (45, "TEXT('Customer')"),
        ]),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer')"),
        fld("NAME", "30", "A", "", "TEXT('Name')"),
        fld("BAL", "9", "P", "2", "DFT(0)"),
        fld("SINCE", "", "L", "", "ALWNULL"),
        fld("NOTE", "100", "A", "", "VARLEN(20) ALWNULL"),
        at(&[(6, "A"), (17, "K"), (19, "CUSNO")]),
    ];
    let (source, _) = Source::from_text(&src.join("\n"));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    c.definition.unwrap_or_else(|| panic!("{}", c.listing))
}

fn record(cusno: u32, who: &str) -> Vec<u8> {
    // CUSNO P(7,0) 4 bytes, NAME 30, BAL P(9,2) 5, SINCE 10, NOTE 2 + 100.
    let mut r = Vec::new();
    let p = qdecimal::Packed::new(7, 0).unwrap();
    r.extend(
        p.encode(
            &qdecimal::Decimal::new(i128::from(cusno), 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap(),
    );
    let mut nm = [ccsid::SPACE; 30];
    ccsid::encode_fixed(who, &mut nm).unwrap();
    r.extend(nm);
    r.extend(
        qdecimal::Packed::new(9, 2)
            .unwrap()
            .encode(
                &qdecimal::Decimal::parse("12.50").unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    r.extend(ccsid::to_ebcdic("2026-10-10").unwrap());
    let note = ccsid::to_ebcdic("hello").unwrap();
    r.extend(u16::try_from(note.len()).unwrap().to_be_bytes());
    r.extend(&note);
    r.extend(std::iter::repeat_n(ccsid::SPACE, 100 - note.len()));
    r
}

#[test]
fn create_describe_and_members() {
    let (d, mut store, lib) = open_new();
    let def = customer_def();
    let file = {
        let mut t = store.begin().unwrap();
        let opts = CreateFileOpts {
            text: "Customer master".into(),
            now: 1_767_225_700,
            reusedlt: true,
            max_members: 3,
            ..CreateFileOpts::default()
        };
        let id = file::create_pf(&mut t, lib, name("CUSTOMER"), &def, &opts).unwrap();
        t.commit().unwrap();
        id
    };
    {
        let mut t = store.begin().unwrap();
        let desc = file::describe(&mut t, file).unwrap();
        assert_eq!(desc.name().to_string(), "CUSTOMER");
        assert_eq!(desc.attr.kind, FileKind::Physical);
        assert_eq!(desc.attr.record_len, 4 + 30 + 5 + 10 + 102);
        assert_eq!(desc.attr.format_count, 1);
        assert_eq!(desc.attr.member_count, 1);
        assert!(desc.attr.reusedlt);
        assert_eq!(desc.attr.size, Size::default());
        let f = &desc.formats[0];
        assert_eq!(f.format.name.to_string(), "CUSREC");
        assert_eq!(f.format.text.as_deref(), Some("Customer"));
        assert_eq!(f.level_id, db::fmt_level_id(&def.format.fields));
        assert_eq!(f.level_id.len(), 13);
        assert_eq!(f.null_map_len, 1);
        assert_eq!(f.key_len, 8);
        assert_eq!(f.unique, Some(dds::NullKeys::Include));
        assert_eq!(f.format.fields.len(), 5);
        assert_eq!(f.format.fields[1].text.as_deref(), Some("Name"));
        assert_eq!(f.format.fields[0].colhdg, vec!["Customer"]);
        assert_eq!(
            f.format.fields[2].default,
            Some(dds::DefaultValue::Number("0".into()))
        );
        assert!(f.format.fields[3].allow_null);
        assert_eq!(f.format.fields[3].date_fmt, Some(dds::DateFmt::Iso));
        assert_eq!(f.format.fields[3].date_sep, Some('-'));
        assert_eq!(f.format.fields[4].varlen.unwrap().allocated, Some(20));
        assert_eq!(f.format.fields[4].byte_len, 102);
        assert_eq!(f.format.fields[4].offset, 49);
        assert_eq!(f.format.keys.len(), 1);
        assert_eq!(f.format.keys[0].name.to_string(), "CUSNO");
        assert_eq!(f.null_index(3), Some(0));
        assert_eq!(f.null_index(4), Some(1));
        assert_eq!(f.null_index(0), None);
        // The compiled fields survive the trip, less what the file drops.
        let mut want = def.format.fields.clone();
        for (i, w) in want.iter_mut().enumerate() {
            w.line = i;
            w.validity.clear();
            w.reference = None;
        }
        assert_eq!(f.format.fields, want);
        assert_eq!(desc.members.len(), 1);
        assert_eq!(desc.members[0].name.to_string(), "CUSTOMER");
        assert_eq!(desc.members[0].object.type_code, types::MBR);
        assert_eq!(
            desc.member(MemberSpec::First).unwrap().name.to_string(),
            "CUSTOMER"
        );
        // Members: add up to the maximum, rename, remove.
        file::add_member(&mut t, file, name("JAN"), "January", None, 2).unwrap();
        file::add_member(&mut t, file, name("FEB"), "February", None, 3).unwrap();
        assert!(matches!(
            file::add_member(&mut t, file, name("MAR"), "", None, 4),
            Err(DbError::TooManyMembers { max: 3, .. })
        ));
        assert!(matches!(
            file::add_member(&mut t, file, name("JAN"), "", None, 4),
            Err(DbError::TooManyMembers { .. } | DbError::MemberExists { .. })
        ));
        file::rename_member(&mut t, file, name("FEB"), name("FEBRUARY"), 5).unwrap();
        file::remove_member(&mut t, JobHandle(1), file, name("JAN")).unwrap();
        let desc = file::describe(&mut t, file).unwrap();
        let names: Vec<String> = desc.members.iter().map(|m| m.name.to_string()).collect();
        assert_eq!(names, ["CUSTOMER", "FEBRUARY"]);
        assert_eq!(
            desc.member(MemberSpec::Last).unwrap().name.to_string(),
            "FEBRUARY"
        );
        assert!(matches!(
            file::remove_member(&mut t, JobHandle(1), file, name("JAN")),
            Err(DbError::MemberNotFound { .. })
        ));
        let feb = desc
            .member(MemberSpec::Name(name("FEBRUARY")))
            .unwrap()
            .object;
        let (_, h) = ops::read_header(&mut t, feb).unwrap();
        assert_eq!(h.name.to_string(), "FEBRUARY");
        assert_eq!(h.library, file);
        t.commit().unwrap();
    }
    // The member table grows past its initial capacity.
    {
        let mut t = store.begin().unwrap();
        let mut desc = file::describe(&mut t, file).unwrap();
        desc.attr.max_members = 0;
        ops::write_attr(&mut t, desc.page, 0, &desc.attr.encode()).unwrap();
        for i in 0..20 {
            file::add_member(&mut t, file, name(&format!("M{i}")), "", None, 10).unwrap();
        }
        let desc = file::describe(&mut t, file).unwrap();
        assert_eq!(desc.members.len(), 22);
        assert!(desc.attr.member_cap >= 22);
        assert_eq!(desc.members[21].name.to_string(), "M19");
        t.commit().unwrap();
    }
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let desc = file::describe(&mut t, file).unwrap();
    assert_eq!(desc.members.len(), 22);
    assert_eq!(desc.formats[0].format.fields.len(), 5);
    t.commit().unwrap();
    let report = store.reclaim(ReclaimMode::Check, 1_767_300_000).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
}

#[test]
fn records_in_a_member() {
    let (d, mut store, lib) = open_new();
    let def = customer_def();
    let (file, mbr) = {
        let mut t = store.begin().unwrap();
        let opts = CreateFileOpts {
            now: 1,
            reusedlt: true,
            size: Size {
                initial: 5,
                increment: 2,
                max_increments: 1,
            },
            ..CreateFileOpts::default()
        };
        let id = file::create_pf(&mut t, lib, name("CUST"), &def, &opts).unwrap();
        let desc = file::describe(&mut t, id).unwrap();
        let mbr = desc.members[0].object;
        t.commit().unwrap();
        (id, mbr)
    };
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    assert_eq!(m.core.record_len(), 151);
    assert_eq!(m.core.null_map_len(), 1);
    assert_eq!(m.attr().max_records, 7);
    assert!(m.attr().reuse_deleted);
    assert_eq!(m.read(&mut t, 1).unwrap(), None);
    for i in 1..=5u32 {
        let rrn = m
            .write_new(&mut t, &record(i, &format!("Customer {i}")), &[0])
            .unwrap();
        assert_eq!(rrn, u64::from(i));
    }
    assert_eq!(m.attr().record_count, 5);
    assert_eq!(m.attr().slots, 5);
    let s3 = m.read(&mut t, 3).unwrap().unwrap();
    assert_eq!(s3.record, record(3, "Customer 3"));
    assert!(!s3.deleted);
    assert!(!s3.is_null(0));
    // Null map and update.
    m.update(&mut t, 3, &record(3, "Updated 3"), &[0b01])
        .unwrap();
    let s3 = m.read(&mut t, 3).unwrap().unwrap();
    assert_eq!(s3.record, record(3, "Updated 3"));
    assert!(s3.is_null(0));
    assert!(!s3.is_null(1));
    // Bad lengths.
    assert!(matches!(
        m.write_new(&mut t, &[0; 10], &[0]),
        Err(DbError::RecordLength { want: 151, got: 10 })
    ));
    assert!(matches!(
        m.write_new(&mut t, &record(9, "x"), &[]),
        Err(DbError::RecordLength { want: 1, got: 0 })
    ));
    // Delete and reuse.
    m.delete(&mut t, 2).unwrap();
    m.delete(&mut t, 4).unwrap();
    assert_eq!(m.attr().record_count, 3);
    assert_eq!(m.attr().deleted_count, 2);
    assert!(m.read(&mut t, 2).unwrap().unwrap().deleted);
    assert!(matches!(
        m.delete(&mut t, 2),
        Err(DbError::RecordDeleted { rrn: 2 })
    ));
    assert!(matches!(
        m.update(&mut t, 2, &record(2, "x"), &[0]),
        Err(DbError::RecordDeleted { rrn: 2 })
    ));
    assert!(matches!(
        m.delete(&mut t, 9),
        Err(DbError::RecordNotFound { rrn: 9 })
    ));
    assert_eq!(
        m.write_new(&mut t, &record(6, "Reused 2"), &[0]).unwrap(),
        2
    );
    assert_eq!(
        m.write_new(&mut t, &record(7, "Reused 4"), &[0]).unwrap(),
        4
    );
    assert_eq!(m.attr().deleted_count, 0);
    assert_eq!(m.attr().slots, 5);
    // The scan skips nothing now; after deletes it skips the holes.
    let all = m.scan(&mut t, 1, 100).unwrap();
    assert_eq!(
        all.iter().map(|s| s.rrn).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    m.delete(&mut t, 5).unwrap();
    assert_eq!(m.scan(&mut t, 3, 100).unwrap().len(), 2);
    // SIZE(5 2 1) allows 7 slots.
    assert_eq!(m.write_new(&mut t, &record(8, "Six"), &[0]).unwrap(), 5);
    assert_eq!(m.write_new(&mut t, &record(9, "Seven"), &[0]).unwrap(), 6);
    assert_eq!(m.write_new(&mut t, &record(10, "Eight"), &[0]).unwrap(), 7);
    assert!(matches!(
        m.write_new(&mut t, &record(11, "Nine"), &[0]),
        Err(DbError::MemberFull { records: 7, .. })
    ));
    t.commit().unwrap();
    // Persistence, then CLRPFM.
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    assert_eq!(m.attr().record_count, 7);
    assert_eq!(
        m.read(&mut t, 7).unwrap().unwrap().record,
        record(10, "Eight")
    );
    m.clear(&mut t).unwrap();
    assert_eq!(m.attr().slots, 0);
    assert_eq!(m.read(&mut t, 1).unwrap(), None);
    assert_eq!(m.write_new(&mut t, &record(1, "Fresh"), &[0]).unwrap(), 1);
    t.commit().unwrap();
    let report = store.reclaim(ReclaimMode::Check, 2).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
    // Delete the file: members go with it, the store stays clean.
    let mut t = store.begin().unwrap();
    file::delete_file(&mut t, JobHandle(3), file).unwrap();
    assert!(ops::read_header(&mut t, mbr).is_err());
    t.commit().unwrap();
    let report = store.reclaim(ReclaimMode::Check, 3).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
}

#[test]
fn source_and_rcdlen_files() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let opts = CreateFileOpts {
        member: MemberOpt::None,
        max_members: 0,
        ..CreateFileOpts::default()
    };
    let src = file::create_srcpf(&mut t, lib, name("QDDSSRC"), 112, &opts).unwrap();
    let desc = file::describe(&mut t, src).unwrap();
    assert_eq!(desc.attr.kind, FileKind::Source);
    assert_eq!(desc.attr.record_len, 112);
    assert!(desc.members.is_empty());
    let f = &desc.formats[0].format;
    let shapes: Vec<String> = f.fields.iter().map(dds::FieldDef::shape).collect();
    assert_eq!(shapes, ["S 6,2", "S 6,0", "A 100"]);
    file::add_member(
        &mut t,
        src,
        name("CUSTOMER"),
        "Customer DDS",
        Some(name("PF")),
        9,
    )
    .unwrap();
    let desc = file::describe(&mut t, src).unwrap();
    assert_eq!(desc.members[0].srctype.unwrap().to_string(), "PF");
    let flat =
        file::create_pf_rcdlen(&mut t, lib, name("FLAT"), 80, &CreateFileOpts::default()).unwrap();
    let desc = file::describe(&mut t, flat).unwrap();
    assert_eq!(desc.formats[0].format.name.to_string(), "FLAT");
    assert_eq!(desc.formats[0].format.fields[0].shape(), "A 80");
    assert_eq!(desc.members.len(), 1);
    assert!(matches!(
        file::describe(&mut t, lib),
        Err(DbError::NotAFile { .. })
    ));
    t.commit().unwrap();
}
