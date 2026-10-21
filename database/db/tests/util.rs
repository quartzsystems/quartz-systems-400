// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The utilities: `CLRPFM`, `RGZPFM` in arrival and key orders, `CPYF`
//! with its member, format and selection options, `CPYTOIMPF` and
//! `CPYFRMIMPF` round trips in delimited and fixed form.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use common::{customer_def, fld, key, lf, name, open_new, pf, rec, record, record_with};
use db::file::{self, CreateFileOpts, FileKind};
use db::odp::{self, Odp, OpenOpts};
use db::util::cpyf::{CompareOp, Conj, IncChar, IncRel};
use db::util::{
    CpyfOpts, DtaFmt, ErrLvl, FmtOpt, ImpfOpts, KeyFile, MbrOpt, RcdDlm, RmvBlank, Target,
    ToMember, clrpfm, cpyf, cpyfrmimpf, cpytoimpf, rgzpfm,
};
use db::{DbError, Member, Value};
use dds::{DateFmt, FileRef, MapRefs};
use qsname::MemberSpec;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

const JOB: JobHandle = JobHandle(1);

/// (CUSNO, NAME) of every active record in RRN order.
fn rows(t: &mut Txn<'_>, m: &Member) -> Vec<(i64, String)> {
    let fmt = m.format().clone();
    let (ci, cf) = fmt.format.field(&name("CUSNO")).unwrap();
    let (ni, nf) = fmt.format.field(&name("NAME")).unwrap();
    m.scan(t, 1, usize::MAX)
        .unwrap()
        .iter()
        .map(|s| {
            let c = match db::record::read_field(cf, &s.record, &s.null_map, fmt.null_index(ci))
                .unwrap()
            {
                Value::Num(d) => i64::try_from(d.to_i128().unwrap()).unwrap(),
                v => panic!("{v:?}"),
            };
            let n = match db::record::read_field(nf, &s.record, &s.null_map, fmt.null_index(ni))
                .unwrap()
            {
                Value::Char(b) => ccsid::decode_trimmed(&b),
                v => panic!("{v:?}"),
            };
            (c, n)
        })
        .collect()
}

fn create_cust(t: &mut Txn<'_>, lib: ObjectId, fname: &str, rows: &[(i64, &str)]) -> ObjectId {
    let id = file::create_pf(
        t,
        lib,
        name(fname),
        &customer_def(),
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut m = Member::open_by(t, id, MemberSpec::First).unwrap();
    for (n, w) in rows {
        m.write_new(t, &record(*n, w), &[0]).unwrap();
    }
    id
}

/// CUSBYNAME: CUST keyed by NAME.
fn create_by_name(t: &mut Txn<'_>, lib: ObjectId, cust: ObjectId) -> ObjectId {
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let def = lf(&[rec("CUSREC", "PFILE(CUST)"), key("NAME", "")], &refs);
    file::create_lf(
        t,
        lib,
        name("CUSBYNAME"),
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
            now: 2,
            ..CreateFileOpts::default()
        },
    )
    .unwrap()
}

#[test]
fn clrpfm_and_rgzpfm() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = create_cust(
        &mut t,
        lib,
        "CUST",
        &[
            (1, "Zed"),
            (2, "Ann"),
            (3, "Mike"),
            (4, "Amy"),
            (5, "Kim"),
            (6, "Bob"),
        ],
    );
    let by_name = create_by_name(&mut t, lib, cust);
    {
        let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
        m.delete(&mut t, 2).unwrap();
        m.delete(&mut t, 5).unwrap();
    }
    t.commit().unwrap();
    // Arrival order: the gaps close, RRNs are renumbered.
    let mut t = store.begin().unwrap();
    let r = rgzpfm(&mut t, JOB, cust, MemberSpec::First, &KeyFile::None).unwrap();
    assert_eq!((r.records, r.deleted_removed), (4, 2));
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!((m.attr().slots, m.attr().deleted_count), (4, 0));
    assert_eq!(
        rows(&mut t, &m),
        [
            (1, "Zed".into()),
            (3, "Mike".into()),
            (4, "Amy".into()),
            (6, "Bob".into())
        ]
    );
    // The dependent path followed.
    let lm = Member::open_by(&mut t, by_name, MemberSpec::First).unwrap();
    let keys = lm
        .keys(
            &mut t,
            std::ops::Bound::Unbounded,
            std::ops::Bound::Unbounded,
            100,
        )
        .unwrap();
    assert_eq!(keys.iter().map(|k| k.rrn).collect::<Vec<_>>(), [3, 4, 2, 1]);
    t.commit().unwrap();
    // KEYFILE(*FILE): the member's own key, CUSNO.
    let mut t = store.begin().unwrap();
    Member::open_by(&mut t, cust, MemberSpec::First)
        .unwrap()
        .delete(&mut t, 2)
        .unwrap();
    let r = rgzpfm(&mut t, JOB, cust, MemberSpec::First, &KeyFile::File).unwrap();
    assert_eq!(r.records, 3);
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!(
        rows(&mut t, &m),
        [(1, "Zed".into()), (4, "Amy".into()), (6, "Bob".into())]
    );
    // KEYFILE(logical): by NAME.
    let r = rgzpfm(
        &mut t,
        JOB,
        cust,
        MemberSpec::First,
        &KeyFile::Logical {
            file: by_name,
            member: MemberSpec::First,
        },
    )
    .unwrap();
    assert_eq!(r.records, 3);
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!(
        rows(&mut t, &m),
        [(4, "Amy".into()), (6, "Bob".into()), (1, "Zed".into())]
    );
    // Not for logical files.
    let e = rgzpfm(&mut t, JOB, by_name, MemberSpec::First, &KeyFile::None).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    assert_eq!(e.msgid(), Some("CPF2817"));
    // An open path holds the member: another job cannot clear it.
    let locks = odp::new_shared();
    let o = Odp::open(&mut t, locks, cust, OpenOpts::input(JobHandle(7))).unwrap();
    let e = clrpfm(&mut t, JOB, cust, MemberSpec::First).unwrap_err();
    assert!(matches!(e, DbError::FileLocked { holder: 7, .. }));
    o.close(&mut t);
    assert_eq!(clrpfm(&mut t, JOB, cust, MemberSpec::First).unwrap(), 3);
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    assert_eq!(m.attr().record_count, 0);
    let lm = Member::open_by(&mut t, by_name, MemberSpec::First).unwrap();
    assert_eq!(lm.attr().path_entries, 0);
    assert!(t.store().locks().holders(cust).is_empty());
    t.commit().unwrap();
}

#[test]
fn cpyf_members_and_selection() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = create_cust(
        &mut t,
        lib,
        "CUST",
        &[
            (1, "Alice"),
            (2, "Bob"),
            (3, "Carol"),
            (4, "Dave"),
            (5, "Erin"),
        ],
    );
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    // CRTFILE(*YES): the to-file takes the from-file's format and member name.
    let create = Target::Create {
        library: lib,
        name: name("CUST2"),
    };
    let e = cpyf(&mut t, JOB, cust, &create, &CpyfOpts::default()).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &create,
        &CpyfOpts {
            crtfile: true,
            now: 5,
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    let cust2 = r.created.unwrap();
    assert_eq!((r.copied, r.excluded, r.errors), (5, 0, 0));
    assert_eq!(r.to_member, name("CUST"));
    let d2 = file::describe(&mut t, cust2).unwrap();
    let d1 = file::describe(&mut t, cust).unwrap();
    assert_eq!(d2.attr.kind, FileKind::Physical);
    assert_eq!(d2.formats[0].level_id, d1.formats[0].level_id);
    assert_eq!(d2.formats[0].unique, d1.formats[0].unique);
    assert_eq!(d2.members[0].name, name("CUST"));
    let to = Target::File(cust2);
    // MBROPT(*NONE) needs an empty member.
    let e = cpyf(&mut t, JOB, cust, &to, &CpyfOpts::default()).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    // *ADD into a UNIQUE member: duplicates are errors ERRLVL counts.
    let add = CpyfOpts {
        mbropt: MbrOpt::Add,
        fromrcd: Some(2),
        torcd: Some(3),
        ..CpyfOpts::default()
    };
    let e = cpyf(&mut t, JOB, cust, &to, &add).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            errlvl: ErrLvl::NoMax,
            ..add
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.errors), (0, 2));
    // *REPLACE with a key range.
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            fromkey: Some(vec![Value::Num("3".parse().unwrap())]),
            tokey: Some(vec![Value::Num("4".parse().unwrap())]),
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!(r.copied, 2);
    let m2 = Member::open_by(&mut t, cust2, MemberSpec::First).unwrap();
    assert_eq!(rows(&mut t, &m2), [(3, "Carol".into()), (4, "Dave".into())]);
    // *UPDADD: existing keys are updated, the rest added.
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::UpdAdd,
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.errors), (5, 0));
    let m2 = Member::open_by(&mut t, cust2, MemberSpec::First).unwrap();
    assert_eq!(m2.attr().record_count, 5);
    assert_eq!(rows(&mut t, &m2)[0], (3, "Carol".into()));
    // INCREL: (CUSNO >= 2 and NAME != Carol) or CUSNO == 1.
    let rel = |conj, field: &str, op, value: &str| IncRel {
        conj,
        field: name(field),
        op,
        value: value.into(),
    };
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            increl: vec![
                rel(Conj::If, "CUSNO", CompareOp::Ge, "2"),
                rel(Conj::And, "NAME", CompareOp::Ne, "Carol"),
                rel(Conj::Or, "CUSNO", CompareOp::Eq, "1"),
            ],
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.excluded), (4, 1));
    let m2 = Member::open_by(&mut t, cust2, MemberSpec::First).unwrap();
    assert_eq!(
        rows(&mut t, &m2).iter().map(|r| r.0).collect::<Vec<_>>(),
        [1, 2, 4, 5]
    );
    // INCCHAR on a field, NBRRCDS, a null test.
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            incchar: Some(IncChar {
                field: Some(name("NAME")),
                position: 1,
                op: CompareOp::Eq,
                value: ccsid::to_ebcdic("C").unwrap(),
            }),
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.excluded), (1, 4));
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            nbrrcds: Some(2),
            incchar: Some(IncChar {
                field: Some(name("NAME")),
                position: 1,
                op: CompareOp::Ct,
                value: ccsid::to_ebcdic("e").unwrap(),
            }),
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!(r.copied, 2);
    let m2 = Member::open_by(&mut t, cust2, MemberSpec::First).unwrap();
    assert_eq!(rows(&mut t, &m2), [(1, "Alice".into()), (4, "Dave".into())]);
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            increl: vec![rel(Conj::If, "SINCE", CompareOp::Eq, "*NULL")],
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.excluded), (0, 5));
    // TOMBR names a member that is added.
    let r = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            to_member: ToMember::Named(name("ARCH")),
            ..CpyfOpts::default()
        },
    )
    .unwrap_err();
    // MAXMBRS(1) on the created file refuses a second member.
    assert!(matches!(r, DbError::TooManyMembers { .. }));
    // Both ranges at once, same member, a to-file that is logical.
    let e = cpyf(
        &mut t,
        JOB,
        cust,
        &to,
        &CpyfOpts {
            fromrcd: Some(1),
            fromkey: Some(vec![]),
            ..CpyfOpts::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let e = cpyf(&mut t, JOB, cust, &Target::File(cust), &CpyfOpts::default()).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let by_name = create_by_name(&mut t, lib, cust);
    let e = cpyf(
        &mut t,
        JOB,
        cust,
        &Target::File(by_name),
        &CpyfOpts::default(),
    )
    .unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    // From a logical member: key order.
    let r = cpyf(
        &mut t,
        JOB,
        by_name,
        &to,
        &CpyfOpts {
            mbropt: MbrOpt::Replace,
            fromkey: Some(vec![Value::Char(ccsid::to_ebcdic("Carol").unwrap())]),
            ..CpyfOpts::default()
        },
    )
    .unwrap();
    assert_eq!(r.copied, 3);
    let m2 = Member::open_by(&mut t, cust2, MemberSpec::First).unwrap();
    assert_eq!(
        rows(&mut t, &m2)
            .iter()
            .map(|r| r.1.clone())
            .collect::<Vec<_>>(),
        ["Carol", "Dave", "Erin"]
    );
    assert!(t.store().locks().holders(cust).is_empty());
    assert!(t.store().locks().holders(cust2).is_empty());
    t.commit().unwrap();
}

#[test]
fn cpyf_format_options() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = create_cust(&mut t, lib, "CUST", &[(1, "Alice"), (2, "Bob")]);
    // CUST3: CUSNO and NAME, no BAL, an EXTRA with a default.
    let def3 = pf(&[
        rec("CUSREC3", ""),
        fld("CUSNO", "7", "P", "0", ""),
        fld("NAME", "30", "A", "", ""),
        fld("EXTRA", "5", "A", "", "DFT('X')"),
        fld("WHEN", "", "L", "", "ALWNULL DFT(*NULL)"),
    ]);
    let opts = CreateFileOpts {
        now: 1,
        ..CreateFileOpts::default()
    };
    let cust3 = file::create_pf(&mut t, lib, name("CUST3"), &def3, &opts).unwrap();
    let to3 = Target::File(cust3);
    let with = |fmtopt| CpyfOpts {
        mbropt: MbrOpt::Replace,
        fmtopt,
        ..CpyfOpts::default()
    };
    // *NONE: formats differ. *MAP alone: BAL has no home. *DROP alone: EXTRA has no source.
    for opt in [FmtOpt::None, FmtOpt::Map, FmtOpt::Drop] {
        let e = cpyf(&mut t, JOB, cust, &to3, &with(opt)).unwrap_err();
        assert!(matches!(e, DbError::Copy { .. }), "{opt:?}");
    }
    let r = cpyf(&mut t, JOB, cust, &to3, &with(FmtOpt::MapDrop)).unwrap();
    assert_eq!(r.copied, 2);
    let m3 = Member::open_by(&mut t, cust3, MemberSpec::First).unwrap();
    assert_eq!(rows(&mut t, &m3), [(1, "Alice".into()), (2, "Bob".into())]);
    let s = m3.read(&mut t, 1).unwrap().unwrap();
    let f3 = m3.format().clone();
    let (ei, ef) = f3.format.field(&name("EXTRA")).unwrap();
    let extra = db::record::read_field(ef, &s.record, &s.null_map, f3.null_index(ei)).unwrap();
    assert_eq!(extra, Value::Char(ccsid::to_ebcdic("X    ").unwrap()));
    let (wi, _) = f3.format.field(&name("WHEN")).unwrap();
    assert!(s.is_null(f3.null_index(wi).unwrap()));
    // *NOCHK: bytes; the prefix fields line up, EXTRA holds BAL's bytes.
    let r = cpyf(&mut t, JOB, cust, &to3, &with(FmtOpt::NoChk)).unwrap();
    assert_eq!(r.copied, 2);
    let m3 = Member::open_by(&mut t, cust3, MemberSpec::First).unwrap();
    assert_eq!(rows(&mut t, &m3), [(1, "Alice".into()), (2, "Bob".into())]);
    let s = m3.read(&mut t, 1).unwrap().unwrap();
    assert_eq!(&s.record[34..39], &record(1, "Alice")[34..39]);
    // The other way: *MAP *DROP fills BAL with its DFT(0) and SINCE with blanks.
    let r = cpyf(
        &mut t,
        JOB,
        cust3,
        &Target::File(cust),
        &with(FmtOpt::MapDrop),
    )
    .unwrap();
    assert_eq!(r.copied, 2);
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let s = m.read(&mut t, 1).unwrap().unwrap();
    let f = m.format().clone();
    let (bi, bf) = f.format.field(&name("BAL")).unwrap();
    assert_eq!(
        db::record::read_field(bf, &s.record, &s.null_map, f.null_index(bi)).unwrap(),
        Value::Num("0.00".parse().unwrap())
    );
    // *CVTSRC both ways.
    let src = file::create_srcpf(&mut t, lib, name("QSRC"), 112, &opts).unwrap();
    let e = cpyf(&mut t, JOB, cust3, &Target::File(src), &with(FmtOpt::None)).unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let r = cpyf(
        &mut t,
        JOB,
        cust3,
        &Target::File(src),
        &with(FmtOpt::CvtSrc),
    )
    .unwrap();
    assert_eq!(r.copied, 2);
    let sm = Member::open_by(&mut t, src, MemberSpec::First).unwrap();
    let source_rec = sm.read(&mut t, 2).unwrap().unwrap();
    let sf = sm.format().clone();
    let seq = db::record::read_field(&sf.format.fields[0], &source_rec.record, &[], None).unwrap();
    assert_eq!(seq, Value::Num("2.00".parse().unwrap()));
    let data = db::record::field_bytes(&sf.format.fields[2], &source_rec.record);
    let orig = m3.read(&mut t, 2).unwrap().unwrap();
    assert_eq!(&data[..orig.record.len()], &orig.record[..]);
    let cust4 = file::create_pf(&mut t, lib, name("CUST4"), &def3, &opts).unwrap();
    let r = cpyf(
        &mut t,
        JOB,
        src,
        &Target::File(cust4),
        &with(FmtOpt::CvtSrc),
    )
    .unwrap();
    assert_eq!(r.copied, 2);
    let m4 = Member::open_by(&mut t, cust4, MemberSpec::First).unwrap();
    assert_eq!(m4.read(&mut t, 2).unwrap().unwrap().record, orig.record);
    // Source to source with another record length is a field copy.
    let src2 = file::create_srcpf(&mut t, lib, name("QSRC2"), 92, &opts).unwrap();
    let r = cpyf(&mut t, JOB, src, &Target::File(src2), &with(FmtOpt::None)).unwrap();
    assert_eq!(r.copied, 2);
    // A data mapping error counts against ERRLVL: a 7-digit key into 3 digits.
    let small = pf(&[
        rec("SMALL", ""),
        fld("CUSNO", "3", "P", "0", ""),
        fld("NAME", "30", "A", "", ""),
    ]);
    let smallid = file::create_pf(&mut t, lib, name("SMALL"), &small, &opts).unwrap();
    let big = create_cust(&mut t, lib, "BIG", &[(1, "Ok"), (12345, "Big"), (2, "Ok")]);
    let e = cpyf(
        &mut t,
        JOB,
        big,
        &Target::File(smallid),
        &with(FmtOpt::MapDrop),
    )
    .unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let r = cpyf(
        &mut t,
        JOB,
        big,
        &Target::File(smallid),
        &CpyfOpts {
            errlvl: ErrLvl::Count(1),
            ..with(FmtOpt::MapDrop)
        },
    )
    .unwrap();
    assert_eq!((r.copied, r.errors), (2, 1));
    t.commit().unwrap();
}

/// The customer record with `SINCE` and `NOTE` null (blank and empty).
fn null_record(cusno: i64, who: &str) -> Vec<u8> {
    let mut r = record(cusno, who);
    r[39..49].fill(ccsid::SPACE);
    r[49..51].fill(0);
    r[51..].fill(ccsid::SPACE);
    r
}

#[test]
fn impf_round_trips() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = create_cust(&mut t, lib, "CUST", &[(1, "Alice"), (2, "Bob")]);
    {
        let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
        m.write_new(&mut t, &null_record(3, "Carol"), &[0b11])
            .unwrap();
        m.write_new(&mut t, &record_with(4, "O'Dell \"Dee\"", "-7.05"), &[0])
            .unwrap();
    }
    let empty = file::create_pf(
        &mut t,
        lib,
        name("CUST2"),
        &customer_def(),
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    // Delimited, defaults: NAME padded, nulls as their stored defaults.
    let text = cpytoimpf(&mut t, JOB, cust, MemberSpec::First, &ImpfOpts::default()).unwrap();
    let text = String::from_utf8(text).unwrap();
    let lines: Vec<&str> = text.split("\r\n").collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(
        lines[0],
        format!(
            "1,\"Alice{}\",12.50,\"2026-10-10\",\"hello\"",
            " ".repeat(25)
        )
    );
    assert_eq!(
        lines[2],
        format!("3,\"Carol{}\",12.50,\"\",\"\"", " ".repeat(25))
    );
    assert_eq!(
        lines[3],
        format!(
            "4,\"O'Dell \"\"Dee\"\"{}\",-7.05,\"2026-10-10\",\"hello\"",
            " ".repeat(18)
        )
    );
    // Trimmed, nulls empty, dates in *USA, semicolons and a decimal comma.
    let opts = ImpfOpts {
        rmvblank: RmvBlank::Trailing,
        nulls: true,
        datfmt: Some(DateFmt::Usa),
        flddlm: ';',
        decpnt: ',',
        rcddlm: RcdDlm::Lf,
        ..ImpfOpts::default()
    };
    let text = cpytoimpf(&mut t, JOB, cust, MemberSpec::First, &opts).unwrap();
    let text = String::from_utf8(text).unwrap();
    assert_eq!(
        text,
        "1;\"Alice\";12,50;\"10/10/2026\";\"hello\"\n\
         2;\"Bob\";12,50;\"10/10/2026\";\"hello\"\n\
         3;\"Carol\";12,50;;\n\
         4;\"O'Dell \"\"Dee\"\"\";-7,05;\"10/10/2026\";\"hello\"\n"
    );
    // Back in: the records are byte-identical, nulls included.
    let r = cpyfrmimpf(
        &mut t,
        JOB,
        empty,
        MemberSpec::First,
        text.as_bytes(),
        &opts,
    )
    .unwrap();
    assert_eq!((r.records, r.errors), (4, 0));
    let a = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let b = Member::open_by(&mut t, empty, MemberSpec::First).unwrap();
    let sa = a.scan(&mut t, 1, 10).unwrap();
    let sb = b.scan(&mut t, 1, 10).unwrap();
    assert_eq!(sa, sb);
    // Fixed columns: 8 + 30 + 11 + 10 + 100 per record.
    let fixed = ImpfOpts {
        dtafmt: DtaFmt::Fixed,
        ..ImpfOpts::default()
    };
    let text = cpytoimpf(&mut t, JOB, cust, MemberSpec::First, &fixed).unwrap();
    let text = String::from_utf8(text).unwrap();
    let lines: Vec<&str> = text.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().all(|l| l.chars().count() == 159));
    assert_eq!(&lines[3][..19], "       4O'Dell \"Dee");
    assert_eq!(&lines[3][38..49], "      -7.05");
    clrpfm(&mut t, JOB, empty, MemberSpec::First).unwrap();
    let r = cpyfrmimpf(
        &mut t,
        JOB,
        empty,
        MemberSpec::First,
        text.as_bytes(),
        &fixed,
    )
    .unwrap();
    assert_eq!(r.records, 4);
    let b = Member::open_by(&mut t, empty, MemberSpec::First).unwrap();
    assert_eq!(b.scan(&mut t, 1, 10).unwrap(), sa);
    // Bad records: counted against ERRLVL, then the copy stops.
    let bad = "9,\"Nine\",1.00,\"2026-01-01\",\"\"\r\n\
               10,\"Ten\",abc,\"2026-01-01\",\"\"\r\n\
               11,\"Eleven\",2.00,\"2026-13-01\",\"\"\r\n\
               1,\"Dup\",3.00,\"2026-01-01\",\"\"\r\n\
               12,\"Short\"\r\n";
    let e = cpyfrmimpf(
        &mut t,
        JOB,
        empty,
        MemberSpec::First,
        bad.as_bytes(),
        &ImpfOpts::default(),
    )
    .unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    let b = Member::open_by(&mut t, empty, MemberSpec::First).unwrap();
    assert_eq!(b.attr().record_count, 5);
    let r = cpyfrmimpf(
        &mut t,
        JOB,
        empty,
        MemberSpec::First,
        bad.as_bytes(),
        &ImpfOpts {
            errlvl: ErrLvl::NoMax,
            ..ImpfOpts::default()
        },
    )
    .unwrap();
    // The first run wrote record 9 before it stopped, so now 9 is a duplicate too.
    assert_eq!((r.records, r.errors), (0, 5), "{:?}", r.skipped);
    assert_eq!(
        r.skipped.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert!(r.skipped[1].1.contains("BAL"));
    assert!(r.skipped[2].1.contains("SINCE"));
    assert!(r.skipped[4].1.contains("no field"));
    // Importing into a logical file is refused.
    let by_name = create_by_name(&mut t, lib, cust);
    let e = cpyfrmimpf(
        &mut t,
        JOB,
        by_name,
        MemberSpec::First,
        b"",
        &ImpfOpts::default(),
    )
    .unwrap_err();
    assert!(matches!(e, DbError::Copy { .. }));
    // Exporting one is in its key order.
    let text = cpytoimpf(
        &mut t,
        JOB,
        by_name,
        MemberSpec::First,
        &ImpfOpts {
            rmvblank: RmvBlank::Trailing,
            ..ImpfOpts::default()
        },
    )
    .unwrap();
    let text = String::from_utf8(text).unwrap();
    let firsts: Vec<&str> = text.lines().map(|l| l.split(',').next().unwrap()).collect();
    assert_eq!(firsts, ["1", "2", "3", "4"]);
    assert!(t.store().locks().holders(cust).is_empty());
    t.commit().unwrap();
}
