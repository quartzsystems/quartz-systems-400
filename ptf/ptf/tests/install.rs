// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSPTF` over a cumulative package in memory: load, requisite order,
//! the install types, the group's status before and after; and the
//! group database.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use ptf::apply::{self, Ctx};
use ptf::group::{self, Group, GroupStatus};
use ptf::grpdb::GroupDb;
use ptf::install::{self, InsType, Item};
use ptf::package::{self, Build};
use ptf::{Action, PtfDb, PtfHeader, PtfKind, PtfRef, Status};
use qsname::Name;
use saverst::AllowAll;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[9u8; 32])
}

fn trusted() -> [[u8; 32]; 1] {
    [key().verifying_key().to_bytes()]
}

fn system(seed: u64, objs: &[(&str, &str)]) -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, seed));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let p = |lib, n: &str, ty| CreateParams {
        library: lib,
        name: Name::parse(n).unwrap(),
        type_code: ty,
        attribute: String::new(),
        text: String::new(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    };
    let lib = ops::create(&mut t, &p(qsys, "QSVCLIB", types::LIB)).unwrap();
    for (name, value) in objs {
        dtaara::create(
            &mut t,
            &p(lib, name, types::DTAARA),
            DataAreaType::Char { len: 8 },
            &DataAreaValue::Char((*value).into()),
        )
        .unwrap();
    }
    t.commit().unwrap();
    (s, qsys)
}

fn make(ptf: &str, obj: &str, value: &str, prereqs: &[&str], kind: PtfKind) -> Vec<u8> {
    let (mut s, q) = system(11, &[(obj, value)]);
    let k = key();
    let r = |p: &&str| PtfRef {
        ptf: (*p).into(),
        product: "5770SS1".into(),
    };
    let header = PtfHeader {
        ptf: ptf.into(),
        product: "5770SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
        load: "5050".into(),
        target_release: "V1R0M0".into(),
        kind,
        lic: false,
        migration: false,
        cover_only: false,
        created: NOW,
        prereqs: prereqs.iter().map(r).collect(),
        coreqs: Vec::new(),
        supersedes: Vec::new(),
        exits: Vec::new(),
        object_count: 0,
        store_format: 0,
    };
    let b = Build {
        header,
        cover: format!("{ptf} sets {obj}."),
        objects: vec![("QSVCLIB".into(), obj.into(), types::DTAARA)],
        lic: None,
        lic_b: None,
        key: &k,
        system: "QSBUILD".into(),
    };
    package::build(&mut s, q, &b, NOW).unwrap()
}

fn value(s: &mut Store, q: ObjectId, obj: &str) -> String {
    let mut t = s.begin().unwrap();
    let l = ops::find_library(&mut t, q, Name::parse("QSVCLIB").unwrap()).unwrap().id;
    let id = store::obj::library::Library::load(&mut t, l)
        .unwrap()
        .lookup(&mut t, types::DTAARA, Name::parse(obj).unwrap())
        .unwrap()
        .unwrap();
    let v = match dtaara::read(&mut t, id).unwrap() {
        DataAreaValue::Char(c) => c.trim_end().to_string(),
        other => format!("{other:?}"),
    };
    t.abort();
    v
}

/// The package: QS00012 needs QS00011, QS00013 is delayed; listed
/// dependents first.
fn package() -> Vec<Item> {
    vec![
        Item {
            savf: "QGPL/QS00013".into(),
            bytes: make("QS00013", "OTHER", "late", &[], PtfKind::Delayed),
        },
        Item {
            savf: "QGPL/QS00012".into(),
            bytes: make("QS00012", "LEVEL", "two", &["QS00011"], PtfKind::Immediate),
        },
        Item {
            savf: "QGPL/QS00011".into(),
            bytes: make("QS00011", "LEVEL", "one", &[], PtfKind::Immediate),
        },
    ]
}

fn cum() -> String {
    let g = Group {
        name: "C26303110".into(),
        level: 1,
        text: "Cumulative package".into(),
        ptfs: vec!["QS00011".into(), "QS00012".into(), "QS00013".into()],
        related: Vec::new(),
    };
    group::sign_manifest(&group::manifest_text(&g), &key())
}

fn run(ins: InsType, omit: &[String]) -> (PtfDb, install::Report, Store, ObjectId) {
    let items = package();
    let (mut s, q) = system(12, &[("LEVEL", "base"), ("OTHER", "base")]);
    let mut db = PtfDb::default();
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 1,
    };
    let rep = install::install(&mut cx, &mut db, &items, &trusted(), "V1R0M0", ins, omit).unwrap();
    (db, rep, s, q)
}

#[test]
fn immediate_and_delayed() {
    let (mut db, rep, mut s, q) = run(InsType::ImmediateDelayed, &[]);
    assert_eq!(rep.loaded.len(), 3);
    assert_eq!(rep.applied, ["QS00011", "QS00012"], "requisites first");
    assert_eq!(rep.at_ipl, ["QS00013"]);
    assert!(rep.ipl_needed());
    assert_eq!(value(&mut s, q, "LEVEL"), "two");
    assert_eq!(value(&mut s, q, "OTHER"), "base");
    let mut gdb = GroupDb::default();
    gdb.put("C26303110", &cum());
    assert_eq!(gdb.rows(&db, &trusted())[0].status, GroupStatus::ApplyAtIpl);
    // The IPL applies the delayed one.
    let items = package();
    let pkg = package::open(&items[0].bytes, &trusted()).unwrap();
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 2,
    };
    apply::apply_temp(&mut cx, &mut db, &pkg, &items[0].bytes, &[], true).unwrap();
    assert_eq!(value(&mut s, q, "OTHER"), "late");
    assert_eq!(gdb.rows(&db, &trusted())[0].status, GroupStatus::Installed);
    // A newer level offered is marked; the stored form round-trips.
    gdb.offer("C26303110", 2);
    assert_eq!(gdb.rows(&db, &trusted())[0].newer, Some(2));
    assert_eq!(GroupDb::decode(&gdb.encode()), gdb);
}

#[test]
fn install_types() {
    let (db, rep, _, _) = run(InsType::DelayAll, &[]);
    assert!(rep.applied.is_empty());
    assert_eq!(rep.at_ipl, ["QS00011", "QS00012", "QS00013"]);
    assert!(db.ptfs.values().all(|r| r.action == Action::ApplyAtIpl));

    let (db, rep, mut s, q) = run(InsType::ImmediateOnly, &[]);
    assert_eq!(rep.applied, ["QS00011", "QS00012"]);
    assert_eq!(rep.left, ["QS00013"]);
    assert!(!rep.ipl_needed());
    assert_eq!(db.get("QS00013").unwrap().status, Status::Loaded);
    assert_eq!(value(&mut s, q, "LEVEL"), "two");

    // Omitting a prerequisite refuses its dependent.
    let (db, rep, _, _) = run(InsType::ImmediateDelayed, &["QS00011".to_string()]);
    assert!(db.get("QS00011").is_none());
    assert_eq!(rep.failed[0].0, "QS00012");
    assert_eq!(rep.failed[0].1, "CPF3612");
}
