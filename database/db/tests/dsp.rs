// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display commands, the change commands and source members.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{customer_def, fld, key, lf, name, open_new, pf, rec, record, sel};
use db::chg::{ChgLf, ChgPf, chglf, chgpf, chgpf_srcfile};
use db::dsp::{FdType, dspdbr, dspfd, dspffd, dsppfm, render_char, render_hex};
use db::file::{self, CreateFileOpts, FileKind, Maint, Size};
use db::source::{records, source_of, srcget, srcput};
use db::{DbError, Member};
use dds::{CompileOpts, FileRef, MapRefs, NoRefs, compile_pf};
use qsname::MemberSpec;
use store::Store;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

const JOB: JobHandle = JobHandle(1);
const NOW: u64 = 1_767_225_600;

fn setup(store: &mut Store, lib: ObjectId) -> (ObjectId, ObjectId) {
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &customer_def(),
        &CreateFileOpts {
            now: NOW,
            text: "Customers".into(),
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    {
        let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
        for (n, w) in [(1, "Alice"), (2, "Bob"), (3, "Carol")] {
            m.write_new(&mut t, &record(n, w), &[0]).unwrap();
        }
        m.delete(&mut t, 2).unwrap();
    }
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let def = lf(
        &[
            rec("CUSREC", "PFILE(CUST)"),
            key("NAME", ""),
            sel("CUSNO", "COMP(GT 1)"),
        ],
        &refs,
    );
    let sel_lf = file::create_lf(
        &mut t,
        lib,
        name("CUSSEL"),
        &def,
        &mut |fr: &FileRef| {
            if fr.file == name("CUST") {
                Ok(cust)
            } else {
                Err(DbError::Definition {
                    what: fr.to_string(),
                })
            }
        },
        &CreateFileOpts {
            now: NOW + 60,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    t.commit().unwrap();
    (cust, sel_lf)
}

#[test]
fn displays() {
    let (_d, mut store, lib) = open_new();
    let (cust, sel_lf) = setup(&mut store, lib);
    let mut t = store.begin().unwrap();
    // DSPFD of the physical file.
    let fd = dspfd(&mut t, cust).unwrap();
    assert_eq!((fd.name, fd.library.as_str()), (name("CUST"), "QSTEST"));
    assert_eq!(
        (fd.kind, fd.text.as_str()),
        (FileKind::Physical, "Customers")
    );
    assert_eq!(fd.created, NOW);
    let m = &fd.members[0];
    assert_eq!(
        (m.records, m.deleted, m.slots, m.path_entries),
        (2, 1, 3, 2)
    );
    assert!(m.path_valid && m.based_on.is_empty());
    let all = fd.render(FdType::All);
    assert!(all.starts_with("File description\nFile . . . . . . . . . . . . . . :  CUST\n"));
    assert!(all.contains("Created . . . . . . . . . . . . . :  2026-01-01 00:00:00"));
    assert!(all.contains("REUSEDLT:  *NO"));
    assert!(all.contains("Key field 1 . . . . . . . . . . :  CUSNO  Ascending, signed"));
    assert!(all.contains("Unique key                    UNIQUE:  *YES"));
    assert!(all.contains("Level identifier"));
    assert!(all.contains("Current records . . . . . . . . :  2"));
    assert!(!all.contains("Select/omit"));
    let atr = fd.render(FdType::Atr);
    assert!(atr.starts_with("Attributes\n") && !atr.contains("Members\n"));
    assert!(atr.contains("WAITRCD:  60"));
    // DSPFD of the logical file: based-on and select/omit.
    let lfd = dspfd(&mut t, sel_lf).unwrap();
    assert_eq!(lfd.kind, FileKind::Logical);
    assert_eq!(lfd.based_on, [("QSTEST/CUST".to_string(), name("CUSREC"))]);
    assert_eq!(lfd.members[0].based_on.len(), 1);
    assert_eq!(lfd.members[0].path_entries, 1);
    let s = lfd.render(FdType::Select);
    assert_eq!(
        s,
        "Select/omit\nFormat CUSREC . . . . . . . . . . :  \n  S  CUSNO  COMP(GT 1)\n"
    );
    let j = lfd.render(FdType::Join);
    assert!(j.contains("Based-on file 1 . . . . . . . . . :  QSTEST/CUST  format CUSREC"));
    // DSPFFD.
    let ffd = dspffd(&mut t, cust).unwrap();
    let f = &ffd.formats[0];
    assert_eq!(f.name, name("CUSREC"));
    assert_eq!(f.record_len, 151);
    let by = |n: &str| f.fields.iter().find(|x| x.name == name(n)).unwrap().clone();
    let cusno = by("CUSNO");
    assert_eq!(
        (
            cusno.data_type,
            cusno.length,
            cusno.byte_len,
            cusno.position
        ),
        ('P', 7, 4, 1)
    );
    assert_eq!(cusno.key, Some((1, false)));
    assert_eq!(cusno.colhdg, ["Customer"]);
    assert_eq!(by("NAME").position, 5);
    assert_eq!(by("BAL").default.as_deref(), Some("0"));
    let since = by("SINCE");
    assert!(since.allow_null && since.key.is_none());
    assert_eq!(since.format.as_deref(), Some("*ISO"));
    assert_eq!(by("NOTE").varlen, Some(Some(20)));
    let text = ffd.render();
    assert!(text.contains("Record format . . . . . . . . . . :  CUSREC"));
    assert!(text.contains("  CUSNO           P      7    0   4     1      1"));
    assert!(text.contains("  SINCE           L     10    0  10    40 Y"));
    assert!(text.contains("             COLHDG 1 'Customer'"));
    assert!(text.contains("             VARLEN(20)"));
    // DSPPFM.
    let views = dsppfm(&mut t, cust, MemberSpec::First, 1, 100, true).unwrap();
    assert_eq!(views.len(), 3);
    assert!(views[1].deleted && views[1].rrn == 2);
    assert!(views[0].text().contains("Alice"));
    assert!(views[0].text().starts_with("...."));
    let ch = render_char(&dsppfm(&mut t, cust, MemberSpec::First, 1, 100, false).unwrap());
    assert_eq!(ch.lines().count(), 2);
    assert!(ch.starts_with("       1   ....Alice"));
    let hex = render_hex(&views[..1]);
    assert!(
        hex.starts_with(
            "Record 1\n       0  0000001C C1938983 85404040 40404040  *....Alice       *\n"
        ),
        "{hex}"
    );
    let from2 = dsppfm(&mut t, cust, MemberSpec::First, 2, 1, false).unwrap();
    assert_eq!(from2[0].rrn, 3);
    // Through the logical file: the selected records only.
    let lv = dsppfm(&mut t, sel_lf, MemberSpec::First, 1, 100, false).unwrap();
    assert_eq!(lv.iter().map(|v| v.rrn).collect::<Vec<_>>(), [3]);
    // DSPDBR both ways.
    let dbr = dspdbr(&mut t, cust, None).unwrap();
    assert_eq!(dbr.dependents.len(), 1);
    let d = &dbr.dependents[0];
    assert_eq!(
        (d.library.as_str(), d.file, d.member, d.access_path),
        ("QSTEST", name("CUSSEL"), name("CUSSEL"), true)
    );
    assert!(
        dbr.render()
            .contains("  QSTEST/CUSSEL  member CUSSEL  access path")
    );
    let dbr = dspdbr(&mut t, sel_lf, Some(MemberSpec::First)).unwrap();
    assert_eq!(dbr.member, Some(name("CUSSEL")));
    assert_eq!(dbr.based_on[0].file, name("CUST"));
    assert!(dbr.render().contains("Based-on files"));
    t.commit().unwrap();
}

#[test]
fn changes() {
    let (_d, mut store, lib) = open_new();
    let (cust, sel_lf) = setup(&mut store, lib);
    let mut t = store.begin().unwrap();
    // CHGPF: REUSEDLT builds the free tree so the deleted slot is reused;
    // SIZE caps every member; TEXT changes the object.
    chgpf(
        &mut t,
        JOB,
        cust,
        &ChgPf {
            reusedlt: Some(true),
            size: Some(Size {
                initial: 5,
                increment: 0,
                max_increments: 0,
            }),
            text: Some("Changed".into()),
            maint: Some(Maint::Rebld),
            now: NOW + 100,
            ..ChgPf::default()
        },
    )
    .unwrap();
    let d = file::describe(&mut t, cust).unwrap();
    assert!(d.attr.reusedlt);
    assert_eq!(d.attr.size.initial, 5);
    assert_eq!(d.attr.maint, Maint::Rebld);
    assert_eq!(ccsid::decode_trimmed(&d.header.text), "Changed");
    assert_eq!(d.header.changed, NOW + 100);
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert!(m.attr().reuse_deleted);
    assert_eq!(m.attr().max_records, 5);
    assert_eq!(m.write_new(&mut t, &record(4, "Dave"), &[0]).unwrap(), 2);
    assert_eq!(m.write_new(&mut t, &record(5, "Erin"), &[0]).unwrap(), 4);
    m.write_new(&mut t, &record(6, "Fay"), &[0]).unwrap();
    let e = m.write_new(&mut t, &record(8, "Hal"), &[0]).unwrap_err();
    assert!(matches!(e, DbError::MemberFull { .. }));
    m.delete(&mut t, 5).unwrap();
    chgpf(
        &mut t,
        JOB,
        cust,
        &ChgPf {
            reusedlt: Some(false),
            size: Some(Size {
                initial: 6,
                increment: 0,
                max_increments: 0,
            }),
            ..ChgPf::default()
        },
    )
    .unwrap();
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert!(!m.attr().reuse_deleted);
    // CHGPF on a logical file, CHGLF on a physical one.
    assert!(matches!(
        chgpf(&mut t, JOB, sel_lf, &ChgPf::default()),
        Err(DbError::Definition { .. })
    ));
    assert!(matches!(
        chglf(&mut t, JOB, cust, &ChgLf::default()),
        Err(DbError::Definition { .. })
    ));
    // CHGLF MAINT(*REBLD), a write leaves the path invalid; *IMMED rebuilds it.
    chglf(
        &mut t,
        JOB,
        sel_lf,
        &ChgLf {
            maint: Some(Maint::Rebld),
            text: Some("Selected".into()),
            now: NOW + 200,
            ..ChgLf::default()
        },
    )
    .unwrap();
    Member::open_by(&mut t, cust, MemberSpec::First)
        .unwrap()
        .write_new(&mut t, &record(7, "Gus"), &[0])
        .unwrap();
    let lm = Member::open_by(&mut t, sel_lf, MemberSpec::First).unwrap();
    assert!(lm.attr().path_invalid);
    chglf(
        &mut t,
        JOB,
        sel_lf,
        &ChgLf {
            maint: Some(Maint::Immed),
            ..ChgLf::default()
        },
    )
    .unwrap();
    let lm = Member::open_by(&mut t, sel_lf, MemberSpec::First).unwrap();
    assert!(!lm.attr().path_invalid);
    assert_eq!(lm.attr().path_entries, 4);
    assert_eq!(
        ccsid::decode_trimmed(&file::describe(&mut t, sel_lf).unwrap().header.text),
        "Selected"
    );
    // CHGPF SRCFILE: the same format with new text and headings is taken.
    let mut def = customer_def();
    def.format.text = Some("Customer master".into());
    def.format.fields[1].colhdg = vec!["Customer".into(), "name".into()];
    chgpf_srcfile(&mut t, JOB, cust, &def).unwrap();
    let d = file::describe(&mut t, cust).unwrap();
    assert_eq!(d.formats[0].format.text.as_deref(), Some("Customer master"));
    assert_eq!(d.formats[0].format.fields[1].colhdg.len(), 2);
    assert_eq!(d.attr.record_len, 151);
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!(m.attr().record_count, 5);
    assert_eq!(m.scan(&mut t, 1, 10).unwrap().len(), 5);
    // A changed field or key is refused.
    let mut longer = customer_def();
    longer.format.fields[1].length = 40;
    longer.format.fields[1].byte_len = 40;
    longer.format.lay_out();
    let e = chgpf_srcfile(&mut t, JOB, cust, &longer).unwrap_err();
    assert!(matches!(e, DbError::Definition { .. }));
    let rekeyed = pf(&[
        rec("CUSREC", "TEXT('Customer')"),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer')"),
        fld("NAME", "30", "A", "", "TEXT('Name')"),
        fld("BAL", "9", "P", "2", "DFT(0)"),
        fld("SINCE", "", "L", "", "ALWNULL"),
        fld("NOTE", "100", "A", "", "VARLEN(20) ALWNULL"),
        key("NAME", ""),
    ]);
    let e = chgpf_srcfile(&mut t, JOB, cust, &rekeyed).unwrap_err();
    assert!(matches!(e, DbError::Definition { .. }));
    assert!(t.store().locks().holders(cust).is_empty());
    t.commit().unwrap();
}

#[test]
fn source_members() {
    let (_d, mut store, lib) = open_new();
    let mut t: Txn<'_> = store.begin().unwrap();
    let src = file::create_srcpf(
        &mut t,
        lib,
        name("QDDSSRC"),
        112,
        &CreateFileOpts {
            now: NOW,
            max_members: 0,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    file::add_member(&mut t, src, name("CUST"), "Customer DDS", None, NOW).unwrap();
    let lines = [
        rec("CUSREC", "TEXT('Customer')"),
        fld("CUSNO", "7", "P", "0", ""),
        fld("NAME", "30", "A", "", "TEXT('Nam\u{e9} ✓')"),
        key("CUSNO", ""),
    ];
    let text = lines.join("\n");
    let n = srcput(
        &mut t,
        JOB,
        src,
        MemberSpec::Name(name("CUST")),
        &text,
        Some(name("PF")),
        NOW,
    )
    .unwrap();
    assert_eq!(n, 4);
    let desc = file::describe(&mut t, src).unwrap();
    assert_eq!(desc.members[1].srctype, Some(name("PF")));
    let recs = records(&mut t, src, MemberSpec::Name(name("CUST"))).unwrap();
    assert_eq!(
        recs.iter().map(|r| (r.0, r.1)).collect::<Vec<_>>(),
        [
            (100, 260_101),
            (200, 260_101),
            (300, 260_101),
            (400, 260_101)
        ]
    );
    assert_eq!(recs[0].2.len(), 100);
    // Back as text: the unmappable check mark became a blank, é stayed.
    let back = srcget(&mut t, src, MemberSpec::Name(name("CUST"))).unwrap();
    let got: Vec<&str> = back.lines().collect();
    assert_eq!(got[0], lines[0]);
    assert_eq!(got[2], lines[2].replace('✓', " ").trim_end());
    // And compiled from the member.
    let source = source_of(&mut t, src, MemberSpec::Name(name("CUST"))).unwrap();
    assert_eq!(source.len(), 4);
    assert_eq!(source.lines()[1].seq_text(), "2.00");
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let def = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    assert_eq!(def.format.fields.len(), 2);
    assert!(def.is_keyed());
    // A second put replaces, and a data file is not a source file.
    assert_eq!(
        srcput(
            &mut t,
            JOB,
            src,
            MemberSpec::Name(name("CUST")),
            "one\n",
            None,
            NOW
        )
        .unwrap(),
        1
    );
    assert_eq!(
        srcget(&mut t, src, MemberSpec::Name(name("CUST"))).unwrap(),
        "one\n"
    );
    assert_eq!(
        file::describe(&mut t, src).unwrap().members[1].srctype,
        None
    );
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
    assert!(matches!(
        srcput(&mut t, JOB, cust, MemberSpec::First, "x", None, NOW),
        Err(DbError::Definition { .. })
    ));
    t.commit().unwrap();
}
