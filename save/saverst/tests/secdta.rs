// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVSECDTA` on one system, then on another: the library restored,
//! `RSTUSRPRF`, and `RSTAUT` granting what the profile had.

#![allow(clippy::many_single_char_names)]

use std::io::Cursor;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::secdta::{ProfileOptions, restore_profiles, rstaut, save_security};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use store::btree::BTree;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
use store::obj::{CreateParams, JobHandle, ObjectId, auth, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn params(lib: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library: lib,
        name: Name::parse(n).unwrap(),
        type_code,
        attribute: String::new(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    }
}

fn system(seed: u64) -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, seed));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
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
    (store, qsys)
}

fn header(kind: SaveKind) -> MediaHeader {
    MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind,
        created: NOW,
        stream_id: [2; 16],
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVSECDTA".into(),
        user: "QSECOFR".into(),
    }
}

#[test]
fn authorities_follow_the_profile() {
    let (mut a, qa) = system(1);
    let mut t = a.begin().unwrap();
    let lib = ops::create(&mut t, &params(qa, "QSTEST", types::LIB)).unwrap();
    let da = dtaara::create(
        &mut t,
        &params(lib, "DA1", types::DTAARA),
        DataAreaType::Char { len: 4 },
        &DataAreaValue::Char("abcd".into()),
    )
    .unwrap();
    // BOB with a private authority to DA1, as the security layer keeps
    // it: on the object and in the profile's index (tree slot 1).
    let bob = ops::create(&mut t, &params(qa, "BOB", types::USRPRF)).unwrap();
    let (page, mut h) = auth::load(&mut t, da).unwrap();
    auth::private_set(&mut t, page, &mut h, savfmt::name10("BOB"), Some(store::obj::Authority(0x21)))
        .unwrap();
    let mut ix: BTree<ObjectId, u64> = BTree::new(bob);
    ix.insert(&mut t, da, &0x21).unwrap();
    let (bpage, _) = ops::read_header(&mut t, bob).unwrap();
    ops::set_tree_root(&mut t, bpage, 1, ix.root()).unwrap();
    t.commit().unwrap();

    let mut w = Writer::new(Vec::new(), &header(SaveKind::Security)).unwrap();
    let rep = save_security(&mut a, &AllowAll, qa, NOW, &mut w).unwrap();
    assert_eq!(rep.saved(), 1);
    let (_, sec) = w.finish(0).unwrap();
    let mut w = Writer::new(Vec::new(), &header(SaveKind::Library)).unwrap();
    save_library(&mut a, JobHandle(1), &AllowAll, qa, "QSTEST", &SaveOptions::default(), &mut w)
        .unwrap();
    let (_, libsave) = w.finish(0).unwrap();

    // The other system: the library first, as a recovery does.
    let (mut b, qb) = system(2);
    let mut r = Reader::new(Cursor::new(libsave.as_slice())).unwrap();
    let rr = restore(&mut b, JobHandle(1), &AllowAll, qb, &mut r, &RestoreOptions::default()).unwrap();
    assert_eq!(rr.restored(), 1, "{:?}", rr.objects);
    let mut r = Reader::new(Cursor::new(sec.as_slice())).unwrap();
    let pr = restore_profiles(&mut b, JobHandle(1), &AllowAll, qb, &mut r, &ProfileOptions::default())
        .unwrap();
    assert_eq!(pr.profiles(), 1);
    // Nothing granted until RSTAUT.
    let mut t = b.begin().unwrap();
    let qstest = ops::find_library(&mut t, qb, Name::parse("QSTEST").unwrap()).unwrap().id;
    let da_b = store::obj::library::Library::load(&mut t, qstest)
        .unwrap()
        .lookup(&mut t, types::DTAARA, Name::parse("DA1").unwrap())
        .unwrap()
        .unwrap();
    let (_, h) = ops::read_header(&mut t, da_b).unwrap();
    assert!(auth::private_list(&mut t, &h).unwrap().is_empty());
    t.abort();
    let done = rstaut(&mut b, &AllowAll, qb, &[]).unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!((done[0].restored, done[0].not_restored), (1, 0));
    let mut t = b.begin().unwrap();
    let (_, h) = ops::read_header(&mut t, da_b).unwrap();
    let p = auth::private_list(&mut t, &h).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].bits.0, 0x21);
    t.abort();
    // A second RSTAUT has nothing left to do.
    assert!(rstaut(&mut b, &AllowAll, qb, &[]).unwrap().is_empty());
}
