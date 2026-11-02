// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management objects in a store: the shipped set installs once,
//! reads back, and a payload can grow and shrink.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};
use wm::obj::cls::Cls;
use wm::obj::jobd::JobD;
use wm::obj::{Stamp, find, read, shipped, write};

const NOW: u64 = 1_767_225_600;

fn open_new() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    for lib in ["QGPL", "QUSRSYS"] {
        ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse(lib).unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: String::new(),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
        )
        .unwrap();
    }
    t.commit().unwrap();
    (store, qsys)
}

fn stamp() -> Stamp {
    Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
    }
}

#[test]
fn shipped_install_once_and_read_back() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let made = shipped::install(&mut t, qsys, &stamp()).unwrap();
    assert_eq!(made, 61);
    assert_eq!(shipped::install(&mut t, qsys, &stamp()).unwrap(), 0);
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    let names = NameContext {
        curlib: None,
        libl: vec![qsys],
    };
    let id = find(&mut t, qsys, &names, "QGPL", "QDFTJOBD", types::JOBD).unwrap();
    let d = JobD::decode(&read(&mut t, id).unwrap()).unwrap();
    assert_eq!(d.text, "Default job description");
    let id = find(&mut t, qsys, &names, "QSYS", "QCTL", types::CLS).unwrap();
    let c = Cls::decode(&read(&mut t, id).unwrap()).unwrap();
    assert_eq!(c.runpty, 10);
}

#[test]
fn payload_grows_and_shrinks() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let qgpl = wm::obj::library(&mut t, qsys, "QGPL").unwrap();
    let small = JobD::default();
    let id = wm::obj::create(
        &mut t,
        qgpl,
        Name::parse("X").unwrap(),
        types::JOBD,
        "",
        &stamp(),
        &small.encode(),
    )
    .unwrap();
    let big = JobD {
        rqsdta: "x".repeat(3000),
        inllibl: Some((0..200).map(|i| format!("L{i}")).collect()),
        ..JobD::default()
    };
    write(&mut t, id, &big.encode()).unwrap();
    assert_eq!(JobD::decode(&read(&mut t, id).unwrap()).unwrap(), big);
    write(&mut t, id, &small.encode()).unwrap();
    assert_eq!(JobD::decode(&read(&mut t, id).unwrap()).unwrap(), small);
    t.commit().unwrap();
}

#[test]
fn reply_list_round_trip() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, &stamp()).unwrap();
    let mut l = wm::obj::rpyl::load(&mut t, qsys).unwrap();
    assert_eq!(l.entries.len(), 5);
    l.add(wm::obj::rpyl::RpyEntry {
        seq: 60,
        msgid: "CEE9901".into(),
        cmpdta: "FAILPGM".into(),
        start: 1,
        reply: "C".into(),
        dump: false,
    })
    .unwrap();
    wm::obj::rpyl::save(&mut t, qsys, &l, &stamp()).unwrap();
    let back = wm::obj::rpyl::load(&mut t, qsys).unwrap();
    assert_eq!(back, l);
    t.commit().unwrap();
}

#[test]
fn version_1_spooled_files_upgrade_once() {
    use wm::obj::splf::{self, SplfAttrs};
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let qgpl = ops::find_library(&mut t, qsys, Name::parse("QGPL").unwrap()).unwrap().id;
    let make = |t: &mut store::txn::Txn<'_>, n: &str, data: &[u8]| {
        let id = wm::obj::create(t, qgpl, Name::parse(n).unwrap(), types::SPLF, "QSYSPRT", &stamp(), &[]).unwrap();
        let mut a = SplfAttrs {
            name: "QSYSPRT".into(),
            devtype: "*SCS".into(),
            ..SplfAttrs::default()
        };
        splf::init(t, id, &a).unwrap();
        splf::append(t, id, &mut a, data, 1, 2).unwrap();
        id
    };
    let old = make(&mut t, "SP00000001", &[0x34, 0x4C, 5, 0xC1]);
    // A file made now is version 2, upgraded by nothing.
    assert_eq!(splf::version(&mut t, old).unwrap(), splf::VERSION);
    assert!(!splf::upgrade(&mut t, old, |_, _| unreachable!()).unwrap());
    // Marked version 1, as files written before the published codes are.
    let (space, _) = store::obj::space::Space::load(&mut t, old).unwrap();
    space.write(&mut t, 8, &splf::VERSION_OLD_PP.to_le_bytes()[..2]).unwrap();
    assert!(splf::upgrade(&mut t, old, |a, d| {
        assert_eq!(a.devtype, "*SCS");
        Some(d.iter().map(|&b| if b == 0x4C { 0xC4 } else { b }).collect())
    })
    .unwrap());
    let a = splf::read_attrs(&mut t, old).unwrap();
    assert_eq!(splf::read_data(&mut t, old, &a).unwrap(), [0x34, 0xC4, 5, 0xC1]);
    assert_eq!((splf::version(&mut t, old).unwrap(), a.pages, a.records), (splf::VERSION, 1, 2));
    assert!(!splf::upgrade(&mut t, old, |_, _| unreachable!()).unwrap(), "once only");
    // A version 1 file the rewrite leaves is still marked done.
    let kept = make(&mut t, "SP00000002", &[0xC1]);
    let (space, _) = store::obj::space::Space::load(&mut t, kept).unwrap();
    space.write(&mut t, 8, &splf::VERSION_OLD_PP.to_le_bytes()[..2]).unwrap();
    assert!(!splf::upgrade(&mut t, kept, |_, _| None).unwrap());
    assert_eq!(splf::version(&mut t, kept).unwrap(), splf::VERSION);
    t.commit().unwrap();
}
