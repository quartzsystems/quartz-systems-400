// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The restore matrix: every `OPTION` x `MBROPT` x `ALWOBJDIF` over one
//! library changed after its save. Each cell's outcome is a line of
//! `tests/expected/matrix.txt` (`QS400_UPDATE_SAVE=1` rewrites it); the
//! cells the oracle settled are asserted here too, by rule number of
//! `tools/conformance/oracle/save/primary/README.md`. `ALWOBJDIF(*ALL)`
//! without `MBROPT(*ALL)` is the engine alone: the command refuses it
//! first (`CPD3768`).

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::Member;
use db::file::{self, CreateFileOpts};
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use jrn::journal::{CrtJrnOpts, crtjrn};
use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
use qsname::{MemberSpec, Name};
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::{AllowAll, AllowDiff, MbrOpt, RestoreOptions, RestoreReport, RstOption, SaveOptions, msg};
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::{FormatParams, Store, StoreConfig, Txn};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn params(lib: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library: lib,
        name: name(n),
        type_code,
        attribute: String::new(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: false,
    }
}

/// DDS of a PF with one record format and these (name, length) fields.
fn pf_source(fields: &[(&str, u32)]) -> String {
    let line = |parts: &[(usize, &str)]| {
        let mut s = vec![' '; 80];
        for (col, text) in parts {
            for (i, ch) in text.chars().enumerate() {
                s[col - 1 + i] = ch;
            }
        }
        s.into_iter().collect::<String>().trim_end().to_string()
    };
    let mut lines = vec![line(&[(6, "A"), (17, "R"), (19, "REC")])];
    for (f, len) in fields {
        let l = len.to_string();
        lines.push(line(&[(6, "A"), (19, f), (35 - l.len(), &l), (35, "A")]));
    }
    lines.join("\n")
}

fn create_pf(t: &mut Txn<'_>, lib: ObjectId, n: &str, fields: &[(&str, u32)], when: u64) -> ObjectId {
    let (source, _) = Source::from_text(&pf_source(fields));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let pf = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    file::create_pf(
        t,
        lib,
        name(n),
        &pf,
        &CreateFileOpts {
            now: when,
            max_members: 0,
            ..CreateFileOpts::default()
        },
    )
    .unwrap()
}

fn find(t: &mut Txn<'_>, qsys: ObjectId, lib: &str, n: &str, ty: u16) -> Option<ObjectId> {
    let l = ops::find_library(t, qsys, name(lib)).ok()?.id;
    store::obj::library::Library::load(t, l)
        .unwrap()
        .lookup(t, ty, name(n))
        .unwrap()
}

/// QSTEST saved, then changed: CUST gains a member, FMTA is recreated
/// with another format, DA1 changes owner, DA2 gains a primary group,
/// US1 is deleted, IX0 is left alone. Returns the store, QSYS and the
/// save.
fn scenario() -> (Store, ObjectId, Vec<u8>) {
    let d = Arc::new(MemBlockDev::with_seed(8192, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = s.begin().unwrap();
    let q = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(&mut t, &params(q, "QSTEST", types::LIB)).unwrap();
    let cust = create_pf(&mut t, lib, "CUST", &[("CUSNO", 7), ("NAME", 30)], NOW);
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let mut r = vec![0x40u8; 37];
    ccsid::encode_fixed("0000001", &mut r[..7]).unwrap();
    m.write_new(&mut t, &r, &[]).unwrap();
    create_pf(&mut t, lib, "FMTA", &[("A", 5)], NOW);
    let rcv = crtjrnrcv(
        &mut t,
        lib,
        name("RCV0001"),
        &CrtJrnRcvOpts {
            now: NOW,
            ..CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    crtjrn(
        &mut t,
        lib,
        name("JRN"),
        rcv,
        &CrtJrnOpts {
            now: NOW,
            ..CrtJrnOpts::default()
        },
    )
    .unwrap();
    for n in ["DA1", "DA2"] {
        dtaara::create(
            &mut t,
            &params(lib, n, types::DTAARA),
            DataAreaType::Char { len: 10 },
            &DataAreaValue::Char("saved".into()),
        )
        .unwrap();
    }
    usrspc::create(&mut t, &params(lib, "US1", types::USRSPC), 512, true, 0).unwrap();
    usrspc::create(&mut t, &params(lib, "IX0", types::USRSPC), 512, true, 0).unwrap();
    t.commit().unwrap();

    let header = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::Low,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [1; 16],
        system: "QS400".into(),
        serial: "1000001".into(),
        command: "SAVLIB".into(),
        user: "QSECOFR".into(),
    };
    let mut w = Writer::new(Vec::new(), &header).unwrap();
    let rep = saverst::save_library(&mut s, JobHandle(1), &AllowAll, q, "QSTEST", &SaveOptions::default(), &mut w)
        .unwrap();
    assert_eq!(rep.not_saved(), 0, "{:?}", rep.objects);
    let (_, bytes) = w.finish(0).unwrap();

    let mut t = s.begin().unwrap();
    file::add_member(&mut t, cust, name("EXTRA"), "", None, NOW + 10).unwrap();
    let fmta = find(&mut t, q, "QSTEST", "FMTA", types::FILE).unwrap();
    file::delete_file(&mut t, JobHandle(1), fmta).unwrap();
    create_pf(&mut t, lib, "FMTA", &[("A", 5), ("B", 3)], NOW + 10);
    for (n, f) in [("DA1", 0), ("DA2", 1)] {
        let id = find(&mut t, q, "QSTEST", n, types::DTAARA).unwrap();
        let (page, mut h) = ops::read_header(&mut t, id).unwrap();
        if f == 0 {
            h.owner = savfmt::name10("OTHER");
        } else {
            h.primary_group = savfmt::name10("GRPA");
        }
        ops::write_header(&mut t, page, &h).unwrap();
    }
    let us1 = find(&mut t, q, "QSTEST", "US1", types::USRSPC).unwrap();
    ops::delete(&mut t, JobHandle(1), us1).unwrap();
    t.commit().unwrap();
    (s, q, bytes)
}

const OPTIONS: [(&str, RstOption); 3] = [("*ALL", RstOption::All), ("*NEW", RstOption::New), ("*OLD", RstOption::Old)];
const MBROPTS: [(&str, MbrOpt); 4] = [
    ("*MATCH", MbrOpt::Match),
    ("*ALL", MbrOpt::All),
    ("*NEW", MbrOpt::New),
    ("*OLD", MbrOpt::Old),
];
const ALLOWS: [&str; 5] = ["*NONE", "*OWNER", "*PGP", "*FILELVL", "*ALL"];

struct Cell {
    report: RestoreReport,
    /// Names in QSTEST after the restore.
    files: Vec<String>,
    /// CUST's member count after the restore.
    cust_members: usize,
}

fn run(option: RstOption, mbropt: MbrOpt, allow: &str) -> Cell {
    let (mut s, q, bytes) = scenario();
    let allow = if allow == "*NONE" {
        AllowDiff::default()
    } else {
        AllowDiff::from_values(&[allow.to_string()])
    };
    let mut r = Reader::new(Cursor::new(bytes.as_slice())).unwrap();
    let report = saverst::restore(
        &mut s,
        JobHandle(1),
        &AllowAll,
        q,
        &mut r,
        &RestoreOptions {
            option,
            mbropt,
            allow,
            now: NOW + 100,
            system: "QS400".into(),
            ..RestoreOptions::default()
        },
    )
    .unwrap();
    let mut t = s.begin().unwrap();
    let lib = ops::find_library(&mut t, q, name("QSTEST")).unwrap().id;
    let mut files: Vec<String> = store::obj::library::Library::load(&mut t, lib)
        .unwrap()
        .list(&mut t, Some(types::FILE))
        .unwrap()
        .into_iter()
        .map(|(k, _)| k.name.to_string())
        .collect();
    files.sort();
    let cust = find(&mut t, q, "QSTEST", "CUST", types::FILE).unwrap();
    let cust_members = file::describe(&mut t, cust).unwrap().members.len();
    t.abort();
    Cell {
        report,
        files,
        cust_members,
    }
}

fn line(opt: &str, mbr: &str, allow: &str, c: &Cell) -> String {
    let r = &c.report;
    let mut s = format!(
        "OPTION({opt}) MBROPT({mbr}) ALWOBJDIF({allow}): {} {} restored {} not",
        r.completion(),
        r.restored(),
        r.not_restored()
    );
    for o in &r.objects {
        let _ = write!(s, " {}={}", o.name, o.msgid.unwrap_or("ok"));
    }
    for (m, what) in &r.skipped {
        let what = what.split_whitespace().next().unwrap_or("");
        let _ = write!(s, " skip:{what}={m}");
    }
    let mut notes: Vec<&str> = r.notes.iter().map(|(m, _)| *m).collect();
    notes.sort_unstable();
    notes.dedup();
    if !notes.is_empty() {
        let _ = write!(s, " notes={}", notes.join(","));
    }
    let _ = write!(s, " changes={} files={} CUST-members={}", r.changes, c.files.join(","), c.cust_members);
    s
}

fn msgid(c: &Cell, n: &str) -> Option<&'static str> {
    c.report
        .objects
        .iter()
        .find(|o| o.name == n)
        .unwrap_or_else(|| panic!("{n} not considered: {:?}", c.report.objects))
        .msgid
}

fn has_note(c: &Cell, id: &str) -> bool {
    c.report.notes.iter().any(|(m, _)| *m == id)
}

/// The oracle's rules, on the cells they settle.
fn check_rules(opt: RstOption, mbr: MbrOpt, allow: &str, c: &Cell) {
    let r = &c.report;
    // 6, 8: an existing journal and its attached receiver, always.
    if opt != RstOption::New {
        assert_eq!(msgid(c, "JRN"), Some(msg::JOURNAL_EXISTS));
        assert_eq!(msgid(c, "RCV0001"), Some(msg::RECEIVER_ATTACHED));
    }
    match opt {
        // 2: *NEW leaves what exists (informational); US1 comes back.
        RstOption::New => {
            assert_eq!(msgid(c, "US1"), None);
            assert!(r.skipped.iter().all(|(m, _)| *m == msg::SKIP_EXISTS));
            assert!(r.skipped.iter().any(|(_, w)| w.contains("CUST")));
            assert_eq!(c.cust_members, 2);
        }
        // 2: *OLD skips what is missing.
        RstOption::Old => {
            assert!(r.skipped.iter().any(|(m, w)| *m == msg::SKIP_MISSING && w.contains("US1")));
        }
        _ => {}
    }
    if opt == RstOption::New {
        return;
    }
    // 1: something was refused, so the completion is CPF3773.
    assert_eq!(r.completion(), msg::RESTORED_LIB_PARTIAL);
    // 11: *MATCH with another member list.
    if mbr == MbrOpt::Match {
        assert_eq!(msgid(c, "CUST"), Some(msg::NOT_RESTORED_MEMBERS));
    }
    if mbr == MbrOpt::All && msgid(c, "CUST").is_none() {
        assert_eq!(c.cust_members, 2, "*ALL keeps the member the save lacked");
    }
    // 13: owner and primary group.
    let owner_ok = matches!(allow, "*OWNER" | "*ALL");
    let pgp_ok = matches!(allow, "*PGP" | "*ALL");
    assert_eq!(msgid(c, "DA1"), (!owner_ok).then_some(msg::NOT_RESTORED_OWNER));
    assert_eq!(msgid(c, "DA2"), (!pgp_ok).then_some(msg::NOT_RESTORED_PGP));
    assert_eq!(has_note(c, msg::OWNER_KEPT), owner_ok);
    assert_eq!(has_note(c, msg::PGP_KEPT), pgp_ok);
    // 12: a different format is refused unless *ALL with MBROPT(*ALL),
    // which renames the existing file.
    if allow == "*ALL" && mbr == MbrOpt::All {
        assert_eq!(msgid(c, "FMTA"), None);
        assert!(has_note(c, msg::FILE_RENAMED));
        assert!(c.files.iter().any(|f| f == "FMTA0001"), "{:?}", c.files);
    } else if allow != "*ALL" {
        assert_eq!(msgid(c, "FMTA"), Some(msg::NOT_RESTORED_LEVEL));
        assert!(!c.files.iter().any(|f| f == "FMTA0001"));
    }
    // 4: accepted differences are counted (the command sends CPF3848).
    if owner_ok || pgp_ok {
        assert!(r.changes > 0);
    }
}

#[test]
fn every_option_mbropt_and_alwobjdif() {
    let mut out = String::new();
    for (on, o) in OPTIONS {
        for (mn, m) in MBROPTS {
            for a in ALLOWS {
                let c = run(o, m, a);
                check_rules(o, m, a, &c);
                let _ = writeln!(out, "{}", line(on, mn, a, &c));
            }
        }
    }
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/expected/matrix.txt");
    if std::env::var_os("QS400_UPDATE_SAVE").is_some() {
        std::fs::create_dir_all(expected.parent().unwrap()).unwrap();
        std::fs::write(&expected, &out).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&expected).unwrap_or_default().replace("\r\n", "\n");
    assert_eq!(out, want, "the matrix changed; QS400_UPDATE_SAVE=1 rewrites it");
}
