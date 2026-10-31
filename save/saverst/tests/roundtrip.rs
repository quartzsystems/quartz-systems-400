// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A library saved and restored: under another name, over itself with
//! each `OPTION` and `MBROPT`, with objects in use and owners missing.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::io::Cursor;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::{self, CreateFileOpts};
use db::Member;
use dds::{CompileOpts, MapRefs, NoRefs, Source, compile_lf, compile_pf};
use jrn::journal::{CrtJrnOpts, Journal, crtjrn};
use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
use qsname::{MemberSpec, Name};
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::{
    AllowAll, Hooks, MbrOpt, RestoreOptions, RstOption, SaveOptions, msg, restore, save_library,
};
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, LockState, ObjectId, ops, types, usridx, usrspc};
use store::{FormatParams, Store, StoreConfig, Txn};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
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

fn params(lib: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library: lib,
        name: name(n),
        type_code,
        attribute: String::new(),
        text: format!("{n} text"),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: false,
    }
}

struct Sys {
    store: Store,
    qsys: ObjectId,
}

fn system() -> Sys {
    let d = Arc::new(MemBlockDev::with_seed(32768, 7));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    t.commit().unwrap();
    Sys { store, qsys }
}

fn record(cusno: &str, who: &str) -> Vec<u8> {
    let mut r = vec![0x40u8; 37];
    ccsid::encode_fixed(cusno, &mut r[..7]).unwrap();
    ccsid::encode_fixed(who, &mut r[7..]).unwrap();
    r
}

/// QSTEST: CUST (3 records, journaled to QSTJRN), CUSBYNAME over it,
/// a data area, a user index, a user space.
fn build(sys: &mut Sys) -> ObjectId {
    let mut t = sys.store.begin().unwrap();
    let lib = ops::create(&mut t, &params(sys.qsys, "QSTEST", types::LIB)).unwrap();
    let pf_src = [
        at(&[(6, "A"), (45, "UNIQUE")]),
        at(&[(6, "A"), (17, "R"), (19, "CUSREC")]),
        at(&[(6, "A"), (19, "CUSNO"), (34, "7"), (35, "A")]),
        at(&[(6, "A"), (19, "NAME"), (33, "30"), (35, "A")]),
        at(&[(6, "A"), (17, "K"), (19, "CUSNO")]),
    ];
    let (source, _) = Source::from_text(&pf_src.join("\n"));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let pf = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let cust = file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &pf,
        &CreateFileOpts {
            now: NOW,
            max_members: 0,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    for (n, who) in [("0000003", "Carol"), ("0000001", "Alice"), ("0000002", "Bob")] {
        m.write_new(&mut t, &record(n, who), &[]).unwrap();
    }
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &pf);
    let lf_src = [
        at(&[(6, "A"), (17, "R"), (19, "CUSREC"), (45, "PFILE(QSTEST/CUST)")]),
        at(&[(6, "A"), (17, "K"), (19, "NAME")]),
    ];
    let (source, _) = Source::from_text(&lf_src.join("\n"));
    let c = compile_lf(&source, &refs, &CompileOpts::default());
    let lf = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let mut resolve = |_: &dds::FileRef| Ok(cust);
    file::create_lf(
        &mut t,
        lib,
        name("CUSBYNAME"),
        &lf,
        &mut resolve,
        &CreateFileOpts {
            now: NOW,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let rcv = crtjrnrcv(
        &mut t,
        lib,
        name("QSTRCV001"),
        &CrtJrnRcvOpts {
            now: NOW,
            ..CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    let jid = crtjrn(
        &mut t,
        lib,
        name("QSTJRN"),
        rcv,
        &CrtJrnOpts {
            now: NOW,
            ..CrtJrnOpts::default()
        },
    )
    .unwrap();
    db::journal::strjrnpf(&mut t, JobHandle(1), cust, jid, true, false, NOW).unwrap();
    dtaara::create(
        &mut t,
        &params(lib, "DA1", types::DTAARA),
        DataAreaType::Char { len: 20 },
        &DataAreaValue::Char("saved value".into()),
    )
    .unwrap();
    let ix = usridx::create(&mut t, &params(lib, "IX1", types::USRIDX), true, 4, 1500).unwrap();
    for k in ["BBBB", "AAAA", "CCCC"] {
        let mut e = k.as_bytes().to_vec();
        e.resize(1500, b'x');
        usridx::insert(&mut t, ix, &e).unwrap();
    }
    let us = usrspc::create(&mut t, &params(lib, "US1", types::USRSPC), 9000, true, 0).unwrap();
    usrspc::write(&mut t, us, 8000, b"tail bytes").unwrap();
    t.commit().unwrap();
    lib
}

fn header() -> MediaHeader {
    MediaHeader {
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
    }
}

fn save(sys: &mut Sys, job: u64, opts: &SaveOptions) -> (Vec<u8>, saverst::SaveReport) {
    let mut w = Writer::new(Vec::new(), &header()).unwrap();
    w.parameters("SAVLIB LIB(QSTEST)").unwrap();
    let r = save_library(
        &mut sys.store,
        JobHandle(job),
        &AllowAll,
        sys.qsys,
        "QSTEST",
        opts,
        &mut w,
    )
    .unwrap();
    let (_, bytes) = w.finish(u32::try_from(r.not_saved()).unwrap()).unwrap();
    (bytes, r)
}

fn rst(sys: &mut Sys, bytes: &[u8], hooks: &dyn Hooks, opts: &RestoreOptions) -> saverst::RestoreReport {
    let mut r = Reader::new(Cursor::new(bytes)).unwrap();
    restore(&mut sys.store, JobHandle(1), hooks, sys.qsys, &mut r, opts).unwrap()
}

fn find(t: &mut Txn<'_>, qsys: ObjectId, lib: &str, n: &str, ty: u16) -> Option<ObjectId> {
    let l = ops::find_library(t, qsys, name(lib)).ok()?.id;
    store::obj::library::Library::load(t, l)
        .unwrap()
        .lookup(t, ty, name(n))
        .unwrap()
}

fn names_by_lf(t: &mut Txn<'_>, lf: ObjectId) -> Vec<String> {
    use std::ops::Bound;
    let mut m = Member::open_by(t, lf, MemberSpec::First).unwrap();
    m.ensure_path(t, &mut |_, _| {}).unwrap();
    let mut out = Vec::new();
    for k in m.keys(t, Bound::Unbounded, Bound::Unbounded, usize::MAX).unwrap() {
        let slot = m.read_entry(t, &k).unwrap().unwrap();
        out.push(ccsid::decode_trimmed(&slot.record[7..]));
    }
    out
}

fn opts_to(lib: &str) -> RestoreOptions {
    RestoreOptions {
        target_library: Some(lib.into()),
        now: NOW + 100,
        system: "QS400".into(),
        ..RestoreOptions::default()
    }
}

#[test]
fn restore_to_a_renamed_library() {
    let mut sys = system();
    build(&mut sys);
    let (bytes, rep) = save(
        &mut sys,
        1,
        &SaveOptions {
            now: NOW + 50,
            ..SaveOptions::default()
        },
    );
    assert_eq!(rep.not_saved(), 0, "{:?}", rep.objects);
    assert_eq!(rep.saved(), 7);
    assert_eq!(rep.completion(), msg::SAVED_LIB);
    // Save order: journal, receiver, PF, LF, then the rest.
    let order: Vec<&str> = rep.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(&order[..4], &["QSTJRN", "QSTRCV001", "CUST", "CUSBYNAME"]);

    let r = rst(&mut sys, &bytes, &AllowAll, &opts_to("QSTEST2"));
    assert_eq!(r.not_restored(), 0, "{:?}", r.objects);
    assert_eq!(r.restored(), 7);
    assert_eq!(r.created_libraries, ["QSTEST2"]);

    let mut t = sys.store.begin().unwrap();
    let q = sys.qsys;
    let cust = find(&mut t, q, "QSTEST2", "CUST", types::FILE).unwrap();
    let old = find(&mut t, q, "QSTEST", "CUST", types::FILE).unwrap();
    assert_ne!(cust, old);
    let lf = find(&mut t, q, "QSTEST2", "CUSBYNAME", types::FILE).unwrap();
    assert_eq!(names_by_lf(&mut t, lf), ["Alice", "Bob", "Carol"]);
    // The restored PF lists the restored LF, not the original.
    let desc = file::describe(&mut t, cust).unwrap();
    assert_eq!(desc.dependents.len(), 1);
    assert_eq!(desc.dependents[0].lf_file, lf);
    // A write through the new PF reaches the new LF's path.
    Member::open_by(&mut t, cust, MemberSpec::First)
        .unwrap()
        .write_new(&mut t, &record("0000004", "Aaron"), &[])
        .unwrap();
    assert_eq!(names_by_lf(&mut t, lf), ["Aaron", "Alice", "Bob", "Carol"]);
    // Journaled to the restored journal, which lists it.
    let jid = find(&mut t, q, "QSTEST2", "QSTJRN", saverst::order::JRN).unwrap();
    assert_eq!(desc.attr.journal, jid);
    let j = Journal::load(&mut t, jid).unwrap();
    assert!(j.object(&mut t, cust).unwrap().is_some());
    let rcv = find(&mut t, q, "QSTEST2", "QSTRCV001", 0x0019).unwrap();
    assert_eq!(j.attr.attached, rcv);
    // Data area, user index, user space.
    let da = find(&mut t, q, "QSTEST2", "DA1", types::DTAARA).unwrap();
    assert_eq!(
        dtaara::read(&mut t, da).unwrap(),
        DataAreaValue::Char(format!("{:<20}", "saved value"))
    );
    let ix = find(&mut t, q, "QSTEST2", "IX1", types::USRIDX).unwrap();
    let keys: Vec<Vec<u8>> = usridx::first(&mut t, ix, 10)
        .unwrap()
        .into_iter()
        .map(|e| e[..4].to_vec())
        .collect();
    assert_eq!(keys, [b"AAAA".to_vec(), b"BBBB".to_vec(), b"CCCC".to_vec()]);
    let us = find(&mut t, q, "QSTEST2", "US1", types::USRSPC).unwrap();
    assert_eq!(usrspc::read(&mut t, us, 8000, 10).unwrap(), b"tail bytes");
    // History: saved on the originals, restored on the copies.
    let (_, h) = ops::read_header(&mut t, old).unwrap();
    assert_eq!(h.last_saved, NOW + 50);
    let (_, h) = ops::read_header(&mut t, cust).unwrap();
    assert_eq!(h.last_restored, NOW + 100);
    let (_, h) = ops::read_header(&mut t, da).unwrap();
    assert_eq!(store::obj::header::text_of(&h.text), "DA1 text");
    t.abort();
}

#[test]
fn option_and_mbropt_over_existing_objects() {
    let mut sys = system();
    build(&mut sys);
    let (bytes, _) = save(&mut sys, 1, &SaveOptions::default());
    let q = sys.qsys;
    // Change a record and the data area after the save.
    let mut t = sys.store.begin().unwrap();
    let cust = find(&mut t, q, "QSTEST", "CUST", types::FILE).unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    m.update(&mut t, 1, &record("0000003", "Zed"), &[]).unwrap();
    let da = find(&mut t, q, "QSTEST", "DA1", types::DTAARA).unwrap();
    dtaara::write(&mut t, da, &DataAreaValue::Char("changed".into())).unwrap();
    t.commit().unwrap();

    // *NEW: everything exists, so nothing is restored.
    let r = rst(
        &mut sys,
        &bytes,
        &AllowAll,
        &RestoreOptions {
            option: RstOption::New,
            ..RestoreOptions::default()
        },
    );
    assert_eq!(r.restored(), 0);
    assert_eq!(r.not_restored(), 0);

    // *ALL with *MATCH: the saved contents come back, same IDs.
    let r = rst(&mut sys, &bytes, &AllowAll, &RestoreOptions::default());
    assert_eq!(r.not_restored(), 0, "{:?}", r.objects);
    let mut t = sys.store.begin().unwrap();
    assert_eq!(find(&mut t, q, "QSTEST", "CUST", types::FILE), Some(cust));
    let lf = find(&mut t, q, "QSTEST", "CUSBYNAME", types::FILE).unwrap();
    assert_eq!(names_by_lf(&mut t, lf), ["Alice", "Bob", "Carol"]);
    assert_eq!(
        dtaara::read(&mut t, da).unwrap(),
        DataAreaValue::Char(format!("{:<20}", "saved value"))
    );
    // A member added since the save: *MATCH refuses the file.
    file::add_member(&mut t, cust, name("EXTRA"), "", None, NOW).unwrap();
    t.commit().unwrap();
    let r = rst(&mut sys, &bytes, &AllowAll, &RestoreOptions::default());
    let c = r.objects.iter().find(|o| o.name == "CUST").unwrap();
    assert_eq!(c.msgid, Some(msg::NOT_RESTORED_MEMBERS));
    assert_eq!(r.completion(), msg::RESTORED_LIB_PARTIAL);
    // *ALL restores the saved member and keeps EXTRA.
    let r = rst(
        &mut sys,
        &bytes,
        &AllowAll,
        &RestoreOptions {
            mbropt: MbrOpt::All,
            ..RestoreOptions::default()
        },
    );
    assert_eq!(r.not_restored(), 0, "{:?}", r.objects);
    let mut t = sys.store.begin().unwrap();
    assert_eq!(file::describe(&mut t, cust).unwrap().members.len(), 2);
    t.abort();
}

#[test]
fn objects_in_use_are_not_saved_or_restored() {
    let mut sys = system();
    build(&mut sys);
    let q = sys.qsys;
    let mut t = sys.store.begin().unwrap();
    let da = find(&mut t, q, "QSTEST", "DA1", types::DTAARA).unwrap();
    t.store().locks_mut().lock(JobHandle(9), da, LockState::Exclusive).unwrap();
    t.abort();
    let (bytes, rep) = save(&mut sys, 1, &SaveOptions::default());
    let d = rep.objects.iter().find(|o| o.name == "DA1").unwrap();
    assert_eq!(d.msgid, Some(msg::NOT_SAVED_IN_USE));
    assert_eq!(rep.completion(), msg::SAVED_LIB_PARTIAL);
    // Nothing the save took stays locked.
    let mut t = sys.store.begin().unwrap();
    let us = find(&mut t, q, "QSTEST", "US1", types::USRSPC).unwrap();
    assert!(t.store().locks().holders(us).is_empty());
    let cust = find(&mut t, q, "QSTEST", "CUST", types::FILE).unwrap();
    t.store().locks_mut().lock(JobHandle(9), cust, LockState::SharedRead).unwrap();
    t.abort();
    let r = rst(&mut sys, &bytes, &AllowAll, &RestoreOptions::default());
    let c = r.objects.iter().find(|o| o.name == "CUST").unwrap();
    assert_eq!(c.msgid, Some(msg::NOT_RESTORED_IN_USE));
}

struct Spool;

impl Hooks for Spool {
    fn spool_part(&self, lib: &str, name: &str) -> Option<Vec<u8>> {
        (lib == "QSTEST" && name == "OQ").then(|| b"spooled bytes".to_vec())
    }
}

#[test]
fn output_queues_carry_their_spooled_files() {
    let mut sys = system();
    let lib = build(&mut sys);
    let mut t = sys.store.begin().unwrap();
    ops::create(&mut t, &CreateParams { by_system: true, ..params(lib, "OQ", types::OUTQ) }).unwrap();
    t.commit().unwrap();
    let mut w = Writer::new(Vec::new(), &header()).unwrap();
    let opts = SaveOptions {
        spool: true,
        ..SaveOptions::default()
    };
    save_library(&mut sys.store, JobHandle(1), &Spool, sys.qsys, "QSTEST", &opts, &mut w).unwrap();
    let (_, bytes) = w.finish(0).unwrap();
    let r = rst(&mut sys, &bytes, &AllowAll, &opts_to("QSTEST5"));
    assert_eq!(r.spool, [("QSTEST5".to_string(), "OQ".to_string(), b"spooled bytes".to_vec())]);
}

struct NoProfiles;

impl Hooks for NoProfiles {
    fn profile_exists(&self, _: &mut Txn<'_>, name: &str) -> bool {
        name == "QDFTOWN"
    }
}

#[test]
fn missing_owner_goes_to_the_default_owner() {
    let mut sys = system();
    build(&mut sys);
    let (bytes, _) = save(&mut sys, 1, &SaveOptions::default());
    let r = rst(&mut sys, &bytes, &NoProfiles, &opts_to("QSTEST3"));
    assert_eq!(r.not_restored(), 0, "{:?}", r.objects);
    assert!(r.notes.iter().any(|(m, _)| *m == msg::DEFAULT_OWNER));
    let mut t = sys.store.begin().unwrap();
    let da = find(&mut t, sys.qsys, "QSTEST3", "DA1", types::DTAARA).unwrap();
    let (_, h) = ops::read_header(&mut t, da).unwrap();
    assert_eq!(store::obj::header::text_of(&h.owner), "QDFTOWN");
    t.abort();
}

#[test]
fn a_logical_file_alone_finds_its_physical_file_by_name() {
    let mut sys = system();
    build(&mut sys);
    let (bytes, _) = save(&mut sys, 1, &SaveOptions::default());
    // Restore only the LF into an empty library: it is based on the
    // physical file still in QSTEST.
    let mut t = sys.store.begin().unwrap();
    ops::create(&mut t, &params(sys.qsys, "QSTEST4", types::LIB)).unwrap();
    t.commit().unwrap();
    let r = rst(
        &mut sys,
        &bytes,
        &AllowAll,
        &RestoreOptions {
            select: vec![saverst::ObjSel {
                name: "CUSBYNAME".into(),
                type_code: Some(types::FILE),
            }],
            ..opts_to("QSTEST4")
        },
    );
    assert_eq!(r.restored(), 1, "{:?}", r.objects);
    let mut t = sys.store.begin().unwrap();
    let q = sys.qsys;
    let lf = find(&mut t, q, "QSTEST4", "CUSBYNAME", types::FILE).unwrap();
    assert_eq!(names_by_lf(&mut t, lf), ["Alice", "Bob", "Carol"]);
    let cust = find(&mut t, q, "QSTEST", "CUST", types::FILE).unwrap();
    let deps = file::describe(&mut t, cust).unwrap().dependents;
    assert_eq!(deps.len(), 2, "both logical files are maintained");
    t.abort();
}
