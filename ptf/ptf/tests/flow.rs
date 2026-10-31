// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A PTF built on one system and applied on another: load, temporary
//! apply, remove, permanent apply, prerequisites, supersedes, groups.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use ptf::apply::{self, Ctx, QRPLOBJ};
use ptf::group::{self, GroupStatus};
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

fn system(seed: u64, value: &str) -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, seed));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default())
        .unwrap();
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
    dtaara::create(
        &mut t,
        &p(lib, "LEVEL", types::DTAARA),
        DataAreaType::Char { len: 8 },
        &DataAreaValue::Char(value.into()),
    )
    .unwrap();
    t.commit().unwrap();
    (s, qsys)
}

fn header(ptf: &str, prereqs: &[&str], supersedes: &[&str]) -> PtfHeader {
    let r = |p: &&str| PtfRef {
        ptf: (*p).into(),
        product: "QS00SS1".into(),
    };
    PtfHeader {
        ptf: ptf.into(),
        product: "QS00SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
        load: "5050".into(),
        target_release: "V1R0M0".into(),
        kind: PtfKind::Immediate,
        lic: false,
        migration: false,
        cover_only: false,
        created: NOW,
        prereqs: prereqs.iter().map(r).collect(),
        coreqs: Vec::new(),
        supersedes: supersedes.iter().map(r).collect(),
        exits: Vec::new(),
        object_count: 0,
        store_format: 0,
    }
}

/// A PTF whose object is LEVEL with `value`.
fn make(ptf: &str, value: &str, prereqs: &[&str], supersedes: &[&str]) -> Vec<u8> {
    let (mut s, q) = system(11, value);
    let k = key();
    let b = Build {
        header: header(ptf, prereqs, supersedes),
        cover: format!("{ptf}: sets the level to {value}."),
        objects: vec![("QSVCLIB".into(), "LEVEL".into(), types::DTAARA)],
        lic: None,
        lic_b: None,
        key: &k,
        system: "QSBUILD".into(),
    };
    package::build(&mut s, q, &b, NOW).unwrap()
}

fn level(s: &mut Store, q: ObjectId) -> String {
    let mut t = s.begin().unwrap();
    let l = ops::find_library(&mut t, q, Name::parse("QSVCLIB").unwrap()).unwrap().id;
    let id = store::obj::library::Library::load(&mut t, l)
        .unwrap()
        .lookup(&mut t, types::DTAARA, Name::parse("LEVEL").unwrap())
        .unwrap()
        .unwrap();
    let v = match dtaara::read(&mut t, id).unwrap() {
        DataAreaValue::Char(c) => c.trim_end().to_string(),
        other => format!("{other:?}"),
    };
    t.abort();
    v
}

fn rplobj_count(s: &mut Store, q: ObjectId) -> usize {
    let mut t = s.begin().unwrap();
    let n = ops::find_library(&mut t, q, Name::parse(QRPLOBJ).unwrap())
        .map_or(0, |l| ops::list(&mut t, l.id, None).unwrap().len());
    t.abort();
    n
}

#[test]
fn load_apply_remove_and_make_permanent() {
    let one = make("QS00001", "level1", &[], &[]);
    let pkg = package::open(&one, &trusted()).unwrap();
    assert_eq!(pkg.header.ptf, "QS00001");
    assert!(pkg.cover.contains("level1"));
    assert_eq!(pkg.objects, [("QSVCLIB".into(), "LEVEL".into(), types::DTAARA)]);
    // An untrusted signer is refused.
    let other = [SigningKey::from_bytes(&[1; 32]).verifying_key().to_bytes()];
    assert!(package::open(&one, &other).is_err());

    let (mut s, q) = system(12, "base");
    let mut db = PtfDb::default();
    apply::load(&mut db, &pkg, "QGPL/QS00001", "V1R0M0", NOW).unwrap();
    assert_eq!(db.get("QS00001").unwrap().status, Status::Loaded);
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 1,
    };
    apply::apply_temp(&mut cx, &mut db, &pkg, &one, &[], false).unwrap();
    assert_eq!(db.get("QS00001").unwrap().status, Status::TempApplied);
    assert_eq!(level(&mut s, q), "level1");
    assert_eq!(rplobj_count(&mut s, q), 1, "the old LEVEL waits in QRPLOBJ");

    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 2,
    };
    apply::remove(&mut cx, &mut db, "QS00001", false).unwrap();
    assert_eq!(db.get("QS00001").unwrap().status, Status::TempRemoved);
    assert_eq!(level(&mut s, q), "base");
    assert_eq!(rplobj_count(&mut s, q), 0);

    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 3,
    };
    apply::apply_temp(&mut cx, &mut db, &pkg, &one, &[], false).unwrap();
    apply::apply_perm(&mut cx, &mut db, "QS00001").unwrap();
    assert_eq!(db.get("QS00001").unwrap().status, Status::PermApplied);
    assert_eq!(rplobj_count(&mut s, q), 0);
    assert_eq!(level(&mut s, q), "level1");
    // The database round-trips through its stored text.
    assert_eq!(PtfDb::decode(&db.encode()), db);
}

#[test]
fn prerequisites_supersedes_and_groups() {
    let one = make("QS00001", "level1", &[], &[]);
    let two = make("QS00002", "level2", &["QS00001"], &[]);
    let three = make("QS00003", "level3", &[], &["QS00002"]);
    let (p1, p2, p3) = (
        package::open(&one, &trusted()).unwrap(),
        package::open(&two, &trusted()).unwrap(),
        package::open(&three, &trusted()).unwrap(),
    );
    let (mut s, q) = system(13, "base");
    let mut db = PtfDb::default();
    apply::load(&mut db, &p1, "QGPL/QS00001", "V1R0M0", NOW).unwrap();
    apply::load(&mut db, &p2, "QGPL/QS00002", "V1R0M0", NOW).unwrap();
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW,
    };
    // QS00002 needs QS00001 first.
    let e = apply::apply_temp(&mut cx, &mut db, &p2, &two, &[], false).unwrap_err();
    assert_eq!(e.msgid(), apply::msg::PREREQ);
    apply::apply_temp(&mut cx, &mut db, &p1, &one, &[], false).unwrap();
    apply::apply_temp(&mut cx, &mut db, &p2, &two, &[], false).unwrap();
    assert_eq!(level(&mut s, q), "level2");
    // QS00001 cannot go while QS00002 needs it.
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW,
    };
    assert_eq!(
        apply::remove(&mut cx, &mut db, "QS00001", false).unwrap_err().msgid(),
        apply::msg::NEEDED
    );
    // Loading QS00003 supersedes QS00002.
    apply::load(&mut db, &p3, "QGPL/QS00003", "V1R0M0", NOW).unwrap();
    assert_eq!(db.get("QS00002").unwrap().status, Status::Superseded);
    assert_eq!(db.get("QS00002").unwrap().superseded_by, "QS00003");
    // A group of all three: installed once QS00003 is applied.
    let manifest = group::sign_manifest(
        "name = \"QS99001\"\nlevel = 1\ntext = \"Base fixes\"\nptfs = [\"QS00001\", \"QS00002\", \"QS00003\"]\n",
        &key(),
    );
    let (g, ok) = group::parse(&manifest, &trusted()).unwrap();
    assert!(ok);
    assert_eq!(group::status(&g, ok, &db), GroupStatus::NotInstalled);
    apply::apply_temp(&mut cx, &mut db, &p3, &three, &[], false).unwrap();
    assert_eq!(group::status(&g, ok, &db), GroupStatus::Installed);
    let tampered = manifest.replace("Base fixes", "Other fixes");
    let (_, ok) = group::parse(&tampered, &trusted()).unwrap();
    assert_eq!(group::status(&g, ok, &db), GroupStatus::Unknown);
}

#[test]
fn delayed_ptfs_wait_for_the_ipl() {
    let (mut b, bq) = system(14, "late");
    let k = key();
    let mut h = header("QS00009", &[], &[]);
    h.kind = PtfKind::Delayed;
    let build = Build {
        header: h,
        cover: String::new(),
        objects: vec![("QSVCLIB".into(), "LEVEL".into(), types::DTAARA)],
        lic: Some(b"new kernel image"),
        lic_b: Some(b"new kernel image B"),
        key: &k,
        system: "QSBUILD".into(),
    };
    let bytes = package::build(&mut b, bq, &build, NOW).unwrap();
    let pkg = package::open(&bytes, &trusted()).unwrap();
    assert_eq!(pkg.lic_b.as_deref(), Some(&b"new kernel image B"[..]));
    assert_eq!(pkg.lic.as_deref(), Some(&b"new kernel image"[..]));
    let (mut s, q) = system(15, "base");
    let mut db = PtfDb::default();
    apply::load(&mut db, &pkg, "QGPL/QS00009", "V1R0M0", NOW).unwrap();
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW,
    };
    let a = apply::apply_temp(&mut cx, &mut db, &pkg, &bytes, &[], false).unwrap();
    assert_eq!(a, Action::ApplyAtIpl);
    assert_eq!(level(&mut s, q), "base");
    assert_eq!(apply::pending(&db), [("QS00009".to_string(), Action::ApplyAtIpl)]);
    // The IPL applies it.
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW,
    };
    apply::apply_temp(&mut cx, &mut db, &pkg, &bytes, &[], true).unwrap();
    assert_eq!(level(&mut s, q), "late");
    assert!(apply::pending(&db).is_empty());
}
