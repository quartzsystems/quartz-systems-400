// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Balancing by capacity (`STRASPBAL TYPE(*CAPACITY)`): a unit added to
//! a full ASP takes its share of the objects, which read back the same.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{BALANCE_TOLERANCE, BalanceCursor, FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn params() -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xB4; 16],
        unit_uuids: vec![[1; 16]],
        unit_serials: vec!["SER0".into()],
        wal_pages: 32,
        now: NOW,
    }
}

fn create(library: ObjectId, n: &str) -> CreateParams {
    CreateParams {
        library,
        name: qsname::Name::parse(n).unwrap(),
        type_code: types::USRSPC,
        attribute: String::new(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: false,
    }
}

#[allow(clippy::cast_precision_loss)]
fn percents(store: &Store) -> Vec<f64> {
    store
        .span_fill()
        .iter()
        .map(|&(_, u, p)| u as f64 * 100.0 / p as f64)
        .collect()
}

#[test]
fn a_new_unit_takes_its_share() {
    let dev = Arc::new(MemBlockDev::with_seed(2048, 4));
    let mut store = Store::format(
        vec![Box::new(dev) as Box<dyn BlockDev>],
        &params(),
        StoreConfig::default(),
    )
    .unwrap();
    let qsys = {
        let mut t = store.begin().unwrap();
        let q = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
        t.commit().unwrap();
        q
    };
    for i in 0..20u8 {
        let mut t = store.begin().unwrap();
        let id =
            usrspc::create(&mut t, &create(qsys, &format!("S{i}")), 40 * 4064, false, 0).unwrap();
        usrspc::write(&mut t, id, 0, &[i; 300]).unwrap();
        t.commit().unwrap();
    }
    let extra = Arc::new(MemBlockDev::with_seed(2048, 5));
    store.add_unit(Box::new(extra), [2; 16], "SER1").unwrap();
    let before = percents(&store);
    assert!(before[0] - before[1] > 20.0, "{before:?}");
    let mut cursor = BalanceCursor::default();
    let mut steps = 0;
    while !store.balance_step(&mut cursor, 3).unwrap().done {
        steps += 1;
        assert!(steps < 100, "balancing does not finish");
    }
    let after = percents(&store);
    let avg = f64::midpoint(after[0], after[1]);
    assert!(
        after.iter().all(|p| p - avg <= BALANCE_TOLERANCE + 2.5),
        "{after:?}"
    );
    // The objects read back.
    let mut t = store.begin().unwrap();
    for i in 0..20u8 {
        let lib = store::obj::library::Library::load(&mut t, qsys).unwrap();
        let id = lib
            .lookup(
                &mut t,
                types::USRSPC,
                qsname::Name::parse(&format!("S{i}")).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(usrspc::read(&mut t, id, 0, 300).unwrap(), vec![i; 300]);
    }
    t.abort();
    // Balanced: another step has nothing to do.
    assert!(
        store
            .balance_step(&mut BalanceCursor::default(), 3)
            .unwrap()
            .done
    );
}
