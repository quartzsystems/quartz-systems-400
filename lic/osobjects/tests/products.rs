// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped products' own objects, seeded with the catalog: the HTTP
//! server is installed and checks clean.

use std::cell::RefCell;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use licpgm::directory::check::{Finding, chkprdopt};
use licpgm::directory::{InstalledStatus, installed_programs, software_resources};
use licpgm::{OptionNo, ProductId, SystemInfo};
use lpstore::{Products, stamp};
use qsname::Name;
use store::obj::library::Library;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn system() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 9));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [6; 16],
        unit_uuids: vec![[7; 16]],
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
    for lib in ["QUSRSYS", "QSYS2", "QGPL"] {
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

fn dg1() -> ProductId {
    "QS00DG1".parse().unwrap()
}

fn findings(store: &mut Store, qsys: ObjectId) -> Vec<Finding> {
    let cell = RefCell::new(store);
    let mut p = Products::new(&cell, qsys, stamp(NOW));
    let objs = Products::new(&cell, qsys, stamp(NOW));
    let r = chkprdopt(&mut p, &objs, &dg1(), None, None).unwrap();
    r.loads.into_iter().flat_map(|(_, _, f)| f).collect()
}

#[test]
fn the_http_server_is_installed_and_checks_clean() {
    let (mut store, qsys) = system();
    let mut t = store.begin().unwrap();
    let made = osobjects::products::create_all(&mut t, qsys, &stamp(NOW)).unwrap();
    assert_eq!(made, 4, "QHTTPSVR and QZHBMSG, QRPG and QRPGMSG");
    assert_eq!(
        osobjects::products::create_all(&mut t, qsys, &stamp(NOW)).unwrap(),
        0
    );
    let lib = Library::load(&mut t, qsys)
        .unwrap()
        .lookup(&mut t, types::LIB, Name::parse("QHTTPSVR").unwrap())
        .unwrap()
        .unwrap();
    assert!(
        Library::load(&mut t, lib)
            .unwrap()
            .lookup(&mut t, types::MSGF, Name::parse("QZHBMSG").unwrap())
            .unwrap()
            .is_some()
    );
    t.commit().unwrap();
    assert!(lpstore::seed(&mut store, qsys, NOW).unwrap());
    assert_eq!(findings(&mut store, qsys), Vec::new());

    let cell = RefCell::new(&mut store);
    let p = Products::new(&cell, qsys, stamp(NOW));
    // DSPSFWRSC and GO LICPGM option 10 list it.
    assert!(
        software_resources(&p)
            .iter()
            .any(|r| r.product == dg1() && r.option == OptionNo::BASE)
    );
    let sys = SystemInfo::current("QS40001", "P05");
    let row = installed_programs(&p, &sys)
        .into_iter()
        .find(|r| r.product == dg1() && r.option == OptionNo::BASE)
        .unwrap();
    assert_eq!(row.status, InstalledStatus::Compatible);
}

#[test]
fn without_its_objects_the_http_server_checks_missing() {
    let (mut store, qsys) = system();
    lpstore::seed(&mut store, qsys, NOW).unwrap();
    let f = findings(&mut store, qsys);
    assert!(
        f.iter()
            .any(|f| matches!(f, Finding::Missing(o) if o.name == "QZHBMSG")),
        "{f:?}"
    );
}
