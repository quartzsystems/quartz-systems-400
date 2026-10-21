// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A store in memory with `QSYS`, `QGPL`, `QUSRSYS` and a user library.

#![allow(dead_code)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

pub const NOW: u64 = 1_767_225_600;

pub fn create(t: &mut Txn<'_>, lib: ObjectId, name: &str, type_code: u16, owner: &str) -> ObjectId {
    ops::create(
        t,
        &CreateParams {
            library: lib,
            name: Name::parse(name).unwrap(),
            type_code,
            attribute: String::new(),
            text: String::new(),
            owner: owner.into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap()
}

pub fn open_new() -> (Store, ObjectId) {
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
    for lib in ["QGPL", "QUSRSYS", "APPLIB"] {
        create(&mut t, qsys, lib, types::LIB, "QSECOFR");
    }
    t.commit().unwrap();
    (store, qsys)
}

pub fn lib(t: &mut Txn<'_>, qsys: ObjectId, name: &str) -> ObjectId {
    ops::find_library(t, qsys, Name::parse(name).unwrap())
        .unwrap()
        .id
}
