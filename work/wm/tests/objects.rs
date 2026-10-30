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
