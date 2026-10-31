// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signed programs through a save and restore: `QVFYOBJRST` at each
//! level, `QALWOBJRST`, and `CHKOBJITG`'s check after a change.

#![allow(clippy::many_single_char_names)]

use std::io::Cursor;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use qsname::Name;
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::sign::{self, Integrity};
use saverst::{AllowAll, AllowRestore, Hooks, RestoreOptions, SaveOptions, msg, restore, save_library};
use store::obj::space::Space;
use store::obj::{CreateParams, JobHandle, ObjectId, Signature, ops, types};
use store::{FormatParams, Store, StoreConfig, Txn};

const NOW: u64 = 1_767_225_600;

fn params(lib: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library: lib,
        name: Name::parse(n).unwrap(),
        type_code,
        attribute: "CLLE".into(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    }
}

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

struct Keys;

impl Hooks for Keys {
    fn verify(&self, key_id: [u8; 8], digest: &[u8; 32], sig: &[u8; 64]) -> Option<bool> {
        let public = key().verifying_key().to_bytes();
        (sign::key_id(&public) == key_id).then(|| sign::verify(&public, digest, sig))
    }

    fn system_key(&self, _: [u8; 8]) -> bool {
        true
    }
}

/// QSTEST with a signed program GOOD, the same program TAMPERED whose
/// bytes changed after signing, and an unsigned NOSIG.
fn system() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default())
        .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(&mut t, &params(qsys, "QSTEST", types::LIB)).unwrap();
    for n in ["GOOD", "TAMPERED", "NOSIG"] {
        let id = ops::create(&mut t, &params(lib, n, types::PGM)).unwrap();
        let s = Space::ensure(&mut t, id, 5000, 0).unwrap();
        s.write(&mut t, 0, b"program code").unwrap();
        if n != "NOSIG" {
            let digest = sign::content_digest(&mut t, id).unwrap();
            let public = key().verifying_key().to_bytes();
            let (page, mut h) = ops::read_header(&mut t, id).unwrap();
            h.signature = Some(Signature {
                bytes: sign::sign(&key(), &digest),
                key_id: sign::key_id(&public),
                system: true,
            });
            ops::write_header(&mut t, page, &h).unwrap();
        }
        if n == "TAMPERED" {
            s.write(&mut t, 0, b"patched cod").unwrap();
        }
    }
    t.commit().unwrap();
    (store, qsys)
}

fn saved(store: &mut Store, qsys: ObjectId) -> Vec<u8> {
    let h = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [3; 16],
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVLIB".into(),
        user: "QSECOFR".into(),
    };
    let mut w = Writer::new(Vec::new(), &h).unwrap();
    save_library(store, JobHandle(1), &AllowAll, qsys, "QSTEST", &SaveOptions::default(), &mut w).unwrap();
    w.finish(0).unwrap().1
}

fn outcome(store: &mut Store, qsys: ObjectId, bytes: &[u8], lib: &str, verify: u8, allow: &str) -> Vec<(String, Option<&'static str>)> {
    let mut r = Reader::new(Cursor::new(bytes)).unwrap();
    let opts = RestoreOptions {
        target_library: Some(lib.into()),
        verify,
        allow_restore: AllowRestore::from_value(allow),
        ..RestoreOptions::default()
    };
    let rep = restore(store, JobHandle(1), &Keys, qsys, &mut r, &opts).unwrap();
    let mut v: Vec<_> = rep.objects.into_iter().map(|o| (o.name, o.msgid)).collect();
    v.sort();
    v
}

fn lookup(t: &mut Txn<'_>, qsys: ObjectId, lib: &str, n: &str) -> ObjectId {
    let l = ops::find_library(t, qsys, Name::parse(lib).unwrap()).unwrap().id;
    store::obj::library::Library::load(t, l)
        .unwrap()
        .lookup(t, types::PGM, Name::parse(n).unwrap())
        .unwrap()
        .unwrap()
}

#[test]
fn qvfyobjrst_levels() {
    let (mut s, q) = system();
    let bytes = saved(&mut s, q);
    let sig = Some(msg::NOT_RESTORED_SIGNATURE);
    // 3, the default: unsigned restores, a bad signature does not.
    assert_eq!(
        outcome(&mut s, q, &bytes, "L3", 3, "*ALL *ALWVLDERR"),
        [("GOOD".into(), None), ("NOSIG".into(), None), ("TAMPERED".into(), None)],
        "*ALL in QALWOBJRST allows validation errors"
    );
    assert_eq!(
        outcome(&mut s, q, &bytes, "L3B", 3, "*ALWSYSSTT"),
        [("GOOD".into(), None), ("NOSIG".into(), None), ("TAMPERED".into(), sig)]
    );
    // 5: only good signatures.
    assert_eq!(
        outcome(&mut s, q, &bytes, "L5", 5, "*ALWSYSSTT"),
        [("GOOD".into(), None), ("NOSIG".into(), sig), ("TAMPERED".into(), sig)]
    );
    // 1: nothing is checked.
    assert_eq!(
        outcome(&mut s, q, &bytes, "L1", 1, "*ALWSYSSTT"),
        [("GOOD".into(), None), ("NOSIG".into(), None), ("TAMPERED".into(), None)]
    );
    // The restored good program keeps its signature and checks clean;
    // the tampered one restored at level 1 carries none.
    let public = key().verifying_key().to_bytes();
    let mut t = s.begin().unwrap();
    let good = lookup(&mut t, q, "L1", "GOOD");
    assert_eq!(sign::check(&mut t, good, &[public]).unwrap(), Integrity::Good);
    let bad = lookup(&mut t, q, "L1", "TAMPERED");
    assert_eq!(sign::check(&mut t, bad, &[public]).unwrap(), Integrity::Unsigned);
    // On the original system the tampered program fails CHKOBJITG.
    let orig = lookup(&mut t, q, "QSTEST", "TAMPERED");
    assert_eq!(sign::check(&mut t, orig, &[public]).unwrap(), Integrity::Bad);
    t.abort();
}

#[test]
fn qalwobjrst_refuses_system_state() {
    let (mut s, q) = system();
    let mut t = s.begin().unwrap();
    let g = lookup(&mut t, q, "QSTEST", "NOSIG");
    let (page, mut h) = ops::read_header(&mut t, g).unwrap();
    h.user_state = false;
    ops::write_header(&mut t, page, &h).unwrap();
    t.commit().unwrap();
    let bytes = saved(&mut s, q);
    let out = outcome(&mut s, q, &bytes, "LS", 1, "*NONE");
    assert_eq!(out[1], ("NOSIG".into(), Some(msg::NOT_RESTORED_SYSVAL)));
    let out = outcome(&mut s, q, &bytes, "LS2", 1, "*ALWSYSSTT");
    assert_eq!(out[1], ("NOSIG".into(), None));
}
