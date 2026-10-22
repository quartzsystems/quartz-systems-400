// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RCLSTG: a clean store checks clean; zeroed directory pages are
//! rebuilt from the headers; a bad payload page marks its object
//! damaged; objects of a lost library move to QRCL.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::dtaara::{DataAreaType, DataAreaValue};
use store::obj::{
    CreateParams, JobHandle, LockState, ObjectId, dtaara, ops, types, usridx, usrspc,
};
use store::{FormatParams, ReclaimMode, Store, StoreConfig, StoreError};

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn params(library: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library,
        name: name(n),
        type_code,
        attribute: String::new(),
        text: format!("Object {n}"),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: 1_767_225_700,
        by_system: false,
    }
}

struct Fixture {
    dev: Arc<MemBlockDev>,
    store: Store,
    qsys: ObjectId,
    lib: ObjectId,
    other: ObjectId,
    area: ObjectId,
    space: ObjectId,
    index: ObjectId,
}

fn fixture() -> Fixture {
    let dev = Arc::new(MemBlockDev::with_seed(16384, 5));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [3; 16],
        unit_uuids: vec![[4; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(dev.clone()) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let (qsys, lib, other, area, space, index) = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        let lib = ops::create(&mut t, &params(qsys, "LIB1", types::LIB)).unwrap();
        let other = ops::create(&mut t, &params(qsys, "LIB2", types::LIB)).unwrap();
        let area = dtaara::create(
            &mut t,
            &params(lib, "AREA", 0),
            DataAreaType::Char { len: 100 },
            &DataAreaValue::Char("v".into()),
        )
        .unwrap();
        let space = usrspc::create(&mut t, &params(lib, "SPACE", 0), 20_000, false, 0).unwrap();
        let index = usridx::create(&mut t, &params(other, "IDX", 0), false, 4, 40).unwrap();
        for i in 0..500 {
            usridx::insert(&mut t, index, format!("{i:04}data").as_bytes()).unwrap();
            ops::create(&mut t, &params(other, &format!("D{i}"), types::DTAARA)).unwrap();
        }
        t.commit().unwrap();
        (qsys, lib, other, area, space, index)
    };
    store.asp_superblock_set_qsys(qsys);
    store.checkpoint().unwrap();
    Fixture {
        dev,
        store,
        qsys,
        lib,
        other,
        area,
        space,
        index,
    }
}

fn zero_page(dev: &MemBlockDev, page: u64) {
    dev.write(page, &vec![0u8; 4096]).unwrap();
    dev.flush().unwrap();
}

fn reopen(dev: &Arc<MemBlockDev>) -> Store {
    Store::open(
        vec![Box::new(dev.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0
}

#[test]
fn clean_store_checks_clean() {
    let mut f = fixture();
    let r = f.store.reclaim(ReclaimMode::Check, 1).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
    assert_eq!(r.objects, 3 + 3 + 500);
    assert_eq!(r.libraries, 3);
    assert!(!r.rebuilt);
    assert_eq!(r.free_before, r.free_after);
    let c = f.store.check_pages().unwrap();
    assert!(c.bad_pages.is_empty());
    assert!(c.pages_allocated > 500);
    // A rebuild of a clean store changes nothing visible.
    let r = f.store.reclaim(ReclaimMode::All, 2).unwrap();
    assert!(r.rebuilt);
    assert_eq!(
        r.damaged + r.moved_to_qrcl + r.objdir_missing + r.libdir_missing,
        0,
        "{r}"
    );
    let mut t = f.store.begin().unwrap();
    assert_eq!(
        ops::list(&mut t, f.other, Some(types::DTAARA))
            .unwrap()
            .len(),
        500
    );
    assert_eq!(usridx::describe(&mut t, f.index).unwrap().entry_count, 500);
    assert_eq!(
        usridx::find(&mut t, f.index, usridx::KeyOp::Eq, b"0007", 1)
            .unwrap()
            .len(),
        1
    );
    t.abort();
    let r = f.store.reclaim(ReclaimMode::Check, 3).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
}

#[test]
fn zeroed_directories_are_rebuilt() {
    let mut f = fixture();
    // Find the directory pages: the object directory root and LIB2's
    // directory root.
    let objdir_root = f.store.objdir_root().unwrap();
    let lib2_root = {
        let mut t = f.store.begin().unwrap();
        let page = ops::read_header(&mut t, f.other).unwrap().0;
        let p = t.store().read_page(page).unwrap();
        let attr = store::obj::LibAttr::decode(p.body());
        t.abort();
        attr.dir_root.unwrap()
    };
    drop(f.store);
    zero_page(&f.dev, objdir_root);
    zero_page(&f.dev, lib2_root);
    let mut store = reopen(&f.dev);
    // Lookups fail now.
    {
        let mut t = store.begin().unwrap();
        assert!(ops::describe(&mut t, f.area).is_err());
        t.abort();
    }
    let r = store.reclaim(ReclaimMode::All, 9).unwrap();
    assert!(r.rebuilt);
    assert_eq!(r.bad_pages.len(), 2, "{r}");
    assert_eq!(r.objects, 506);
    assert_eq!(r.damaged, 0);
    assert_eq!(r.moved_to_qrcl, 0);
    // Everything is reachable again, and a check is clean.
    let mut t = store.begin().unwrap();
    assert_eq!(
        dtaara::read(&mut t, f.area).unwrap(),
        DataAreaValue::Char(format!("v{}", " ".repeat(99)))
    );
    assert_eq!(usrspc::describe(&mut t, f.space).unwrap().size, 20_000);
    assert_eq!(
        ops::list(&mut t, f.other, Some(types::DTAARA))
            .unwrap()
            .len(),
        500
    );
    assert_eq!(
        ops::find_library(&mut t, f.qsys, name("LIB1")).unwrap().id,
        f.lib
    );
    let libs: Vec<String> = ops::libraries(&mut t, f.qsys)
        .unwrap()
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    assert_eq!(libs, vec!["LIB1", "LIB2", "QSYS"]);
    assert_eq!(usridx::describe(&mut t, f.index).unwrap().entry_count, 500);
    t.abort();
    let r = store.reclaim(ReclaimMode::Check, 10).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
    assert_eq!(r.leaked_pages, 0);
    // The store survives a reopen after the rebuild.
    drop(store);
    let mut store = reopen(&f.dev);
    let mut t = store.begin().unwrap();
    assert_eq!(ops::describe(&mut t, f.space).unwrap().name, name("SPACE"));
    t.abort();
}

#[test]
fn bad_payload_page_marks_the_object_damaged() {
    let mut f = fixture();
    let data_page = {
        let mut t = f.store.begin().unwrap();
        let h = ops::describe(&mut t, f.space).unwrap();
        t.abort();
        h.extents[0].start + 2
    };
    drop(f.store);
    zero_page(&f.dev, data_page);
    let mut store = reopen(&f.dev);
    let r = store.reclaim(ReclaimMode::Check, 1).unwrap();
    assert_eq!(r.bad_pages.len(), 1, "{r}");
    assert_eq!(r.bad_pages[0].page, data_page);
    assert_eq!(r.damaged, 1);
    assert_eq!(r.problems(), 2, "{r}");
    let r = store.reclaim(ReclaimMode::All, 2).unwrap();
    assert_eq!(r.damaged, 1);
    let mut t = store.begin().unwrap();
    assert!(ops::describe(&mut t, f.space).unwrap().damaged);
    assert!(!ops::describe(&mut t, f.area).unwrap().damaged);
    // The damaged object can still be deleted, which frees its pages.
    ops::delete(&mut t, JobHandle(1), f.space).unwrap();
    t.commit().unwrap();
    let r = store.reclaim(ReclaimMode::Check, 3).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
}

#[test]
fn objects_of_a_lost_library_move_to_qrcl() {
    let mut f = fixture();
    let lib1_page = {
        let mut t = f.store.begin().unwrap();
        let p = ops::read_header(&mut t, f.lib).unwrap().0;
        t.abort();
        p
    };
    drop(f.store);
    zero_page(&f.dev, lib1_page);
    let mut store = reopen(&f.dev);
    let r = store.reclaim(ReclaimMode::All, 5).unwrap();
    assert_eq!(r.moved_to_qrcl, 2, "{r}");
    assert_eq!(r.bad_pages.len(), 1);
    let mut t = store.begin().unwrap();
    let qrcl = ops::find_library(&mut t, f.qsys, name("QRCL")).unwrap().id;
    let in_qrcl: Vec<String> = ops::list(&mut t, qrcl, None)
        .unwrap()
        .iter()
        .map(|(k, _)| k.name.to_string())
        .collect();
    assert_eq!(in_qrcl, vec!["AREA", "SPACE"]);
    assert_eq!(ops::describe(&mut t, f.area).unwrap().library, qrcl);
    assert!(matches!(
        ops::find_library(&mut t, f.qsys, name("LIB1")),
        Err(store::StoreError::LibraryNotFound { .. })
    ));
    t.abort();
    let r = store.reclaim(ReclaimMode::Check, 6).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
}

#[test]
fn a_rebuild_needs_the_store_quiesced() {
    let mut f = fixture();
    // Another job holds an object: no rebuild, but *CHECK still runs.
    f.store
        .locks_mut()
        .lock(JobHandle(2), f.area, LockState::Exclusive)
        .unwrap();
    assert!(matches!(
        f.store.reclaim(ReclaimMode::All, 1),
        Err(StoreError::Busy { objects: 1 })
    ));
    assert!(matches!(f.store.quiesce(), Err(StoreError::Busy { .. })));
    assert!(!f.store.is_quiesced());
    let r = f.store.reclaim(ReclaimMode::Check, 1).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
    f.store.locks_mut().release_job(JobHandle(2));
    // Quiesced by the service: jobs cannot start transactions, the
    // rebuild runs, and the mode stays until released.
    f.store.quiesce().unwrap();
    assert!(matches!(f.store.begin(), Err(StoreError::Quiesced)));
    let r = f.store.reclaim(ReclaimMode::All, 2).unwrap();
    assert!(r.rebuilt);
    assert!(f.store.is_quiesced());
    f.store.release();
    let mut t = f.store.begin().unwrap();
    assert_eq!(usridx::describe(&mut t, f.index).unwrap().entry_count, 500);
    t.abort();
    // A rebuild on its own quiesces for its duration only.
    let r = f.store.reclaim(ReclaimMode::All, 3).unwrap();
    assert!(r.rebuilt && !f.store.is_quiesced());
}
