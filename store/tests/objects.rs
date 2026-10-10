// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object layer: bootstrap, libraries, generic operations, name
//! resolution, locks, storage growth, and all of it across a reopen.

#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::{LibSpec, Name};
use store::obj::header::text_of;
use store::obj::{CreateParams, JobHandle, LockState, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig, StoreError};

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn open_new() -> (Arc<MemBlockDev>, Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(8192, 3));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 64,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let qsys = {
        let mut t = store.begin().unwrap();
        let q = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        t.commit().unwrap();
        q
    };
    (d, store, qsys)
}

fn reopen(d: &Arc<MemBlockDev>) -> Store {
    Store::open(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0
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

#[test]
fn bootstrap_and_libraries() {
    let (d, mut store, qsys) = open_new();
    assert_eq!(qsys.serial, 1);
    assert_eq!(qsys.type_code, types::LIB);
    let (qgpl, qtemp_like) = {
        let mut t = store.begin().unwrap();
        let qgpl = ops::create(&mut t, &params(qsys, "QGPL", types::LIB)).unwrap();
        let mylib = ops::create(&mut t, &params(qsys, "MYLIB", types::LIB)).unwrap();
        let _ = ops::create(&mut t, &params(qsys, "$ACCT", types::LIB)).unwrap();
        assert!(matches!(
            ops::create(&mut t, &params(qsys, "QGPL", types::LIB)),
            Err(StoreError::AlreadyExists { .. })
        ));
        t.commit().unwrap();
        (qgpl, mylib)
    };
    let mut store = {
        drop(store);
        reopen(&d)
    };
    let mut t = store.begin().unwrap();
    let libs: Vec<String> = ops::libraries(&mut t, qsys)
        .unwrap()
        .iter()
        .map(|(n, _)| n.to_string())
        .collect();
    assert_eq!(libs, vec!["$ACCT", "MYLIB", "QGPL", "QSYS"], "EBCDIC order");
    assert_eq!(
        ops::find_library(&mut t, qsys, name("QGPL")).unwrap().id,
        qgpl
    );
    assert!(matches!(
        ops::find_library(&mut t, qsys, name("NOPE")),
        Err(StoreError::LibraryNotFound { .. })
    ));
    let h = ops::describe(&mut t, qtemp_like).unwrap();
    assert_eq!(h.name, name("MYLIB"));
    assert_eq!(h.library, qsys);
    assert_eq!(text_of(&h.text), "Object MYLIB");
    assert_eq!(text_of(&h.owner), "QSECOFR");
    assert_eq!(h.release, release::RELEASE);
    t.abort();
}

#[test]
fn generic_operations_and_resolution() {
    let (d, mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let qgpl = ops::create(&mut t, &params(qsys, "QGPL", types::LIB)).unwrap();
    let mylib = ops::create(&mut t, &params(qsys, "MYLIB", types::LIB)).unwrap();
    let a = ops::create(&mut t, &params(qgpl, "ALPHA", types::DTAARA)).unwrap();
    let b = ops::create(&mut t, &params(qgpl, "BETA", types::DTAARA)).unwrap();
    let q = ops::create(&mut t, &params(qgpl, "ALPHA", types::DTAQ)).unwrap();
    let c = ops::create(&mut t, &params(mylib, "ALPHA", types::DTAARA)).unwrap();
    // Listing: by type, EBCDIC order.
    let l: Vec<(u16, String)> = ops::list(&mut t, qgpl, None)
        .unwrap()
        .iter()
        .map(|(k, _)| (k.type_code, k.name.to_string()))
        .collect();
    assert_eq!(
        l,
        vec![
            (types::DTAARA, "ALPHA".into()),
            (types::DTAARA, "BETA".into()),
            (types::DTAQ, "ALPHA".into())
        ]
    );
    assert_eq!(ops::list(&mut t, qgpl, Some(types::DTAQ)).unwrap().len(), 1);
    // Resolution.
    let ctx = NameContext {
        curlib: Some(mylib),
        libl: vec![mylib, qgpl],
    };
    assert_eq!(
        ops::resolve(
            &mut t,
            qsys,
            &ctx,
            LibSpec::Name(name("QGPL")),
            name("ALPHA"),
            types::DTAARA
        )
        .unwrap()
        .id,
        a
    );
    assert_eq!(
        ops::resolve(
            &mut t,
            qsys,
            &ctx,
            LibSpec::Curlib,
            name("ALPHA"),
            types::DTAARA
        )
        .unwrap()
        .id,
        c
    );
    let found = ops::resolve(
        &mut t,
        qsys,
        &ctx,
        LibSpec::Libl,
        name("ALPHA"),
        types::DTAARA,
    )
    .unwrap();
    assert_eq!(
        (found.id, found.library),
        (c, mylib),
        "first match in list order"
    );
    assert_eq!(
        ops::resolve(
            &mut t,
            qsys,
            &ctx,
            LibSpec::Libl,
            name("BETA"),
            types::DTAARA
        )
        .unwrap()
        .id,
        b
    );
    assert_eq!(
        ops::resolve(
            &mut t,
            qsys,
            &ctx,
            LibSpec::Libl,
            name("ALPHA"),
            types::DTAQ
        )
        .unwrap()
        .id,
        q
    );
    assert!(matches!(
        ops::resolve(
            &mut t,
            qsys,
            &ctx,
            LibSpec::Libl,
            name("GAMMA"),
            types::DTAARA
        ),
        Err(StoreError::ObjectNotFound { .. })
    ));
    // Rename, move, change text.
    ops::rename(&mut t, b, name("GAMMA"), 5).unwrap();
    assert!(matches!(
        ops::rename(&mut t, a, name("GAMMA"), 5),
        Err(StoreError::AlreadyExists { .. })
    ));
    ops::move_to(&mut t, c, qgpl, 6).unwrap_err();
    ops::move_to(&mut t, a, mylib, 6).unwrap_err();
    ops::move_to(&mut t, q, mylib, 6).unwrap();
    ops::change_text(&mut t, a, "Changed", 7).unwrap();
    t.commit().unwrap();
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let h = ops::describe(&mut t, b).unwrap();
    assert_eq!(h.name, name("GAMMA"));
    assert_eq!(h.changed, 5);
    assert_eq!(ops::describe(&mut t, q).unwrap().library, mylib);
    assert_eq!(text_of(&ops::describe(&mut t, a).unwrap().text), "Changed");
    let in_mylib: Vec<String> = ops::list(&mut t, mylib, None)
        .unwrap()
        .iter()
        .map(|(k, _)| format!("{}:{}", k.type_code, k.name))
        .collect();
    assert_eq!(in_mylib, vec!["2:ALPHA", "3:ALPHA"]);
    // Type rules.
    let e = ops::create(&mut t, &params(qgpl, "X", types::SYSVAL)).unwrap_err();
    assert!(matches!(e, StoreError::NotAuthorized { .. }));
    assert!(
        matches!(
            ops::create(&mut t, &params(qgpl, "BOB", types::USRPRF)),
            Err(StoreError::InvalidName { .. })
        ),
        "QSYS only"
    );
    assert!(
        matches!(
            ops::create(&mut t, &params(qsys, "QBOB", types::USRPRF)),
            Err(StoreError::InvalidName { .. })
        ),
        "Q reserved"
    );
    ops::create(&mut t, &params(qsys, "BOB", types::USRPRF)).unwrap();
    ops::create(&mut t, &params(qgpl, "QDTA", types::DTAARA)).unwrap();
    t.commit().unwrap();
}

#[test]
fn delete_locks_and_stale_references() {
    let (_d, mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let lib = ops::create(&mut t, &params(qsys, "LIB1", types::LIB)).unwrap();
    let a = ops::create(&mut t, &params(lib, "A", types::DTAARA)).unwrap();
    let job = JobHandle(1);
    let other = JobHandle(2);
    // A library with objects cannot go.
    assert!(matches!(
        ops::delete(&mut t, job, lib),
        Err(StoreError::Locked { .. })
    ));
    // Another job's lock blocks.
    t.store()
        .locks_mut()
        .lock(other, a, LockState::SharedRead)
        .unwrap();
    assert!(matches!(
        ops::delete(&mut t, job, a),
        Err(StoreError::Locked { .. })
    ));
    t.store().locks_mut().release_job(other);
    ops::delete(&mut t, job, a).unwrap();
    assert!(matches!(
        ops::describe(&mut t, a),
        Err(StoreError::StaleReference { .. })
    ));
    let never = ObjectId {
        asp: 1,
        type_code: types::DTAARA,
        serial: 1_000_000,
        generation: 0,
    };
    assert!(matches!(
        ops::describe(&mut t, never),
        Err(StoreError::ObjectNotFound { .. })
    ));
    assert!(ops::list(&mut t, lib, None).unwrap().is_empty());
    ops::delete(&mut t, job, lib).unwrap();
    assert!(matches!(
        ops::find_library(&mut t, qsys, name("LIB1")),
        Err(StoreError::LibraryNotFound { .. })
    ));
    t.commit().unwrap();
    // Pages all came back: only QSYS's header and directory remain.
    store.checkpoint().unwrap();
    let mut t = store.begin().unwrap();
    assert_eq!(ops::list(&mut t, qsys, None).unwrap().len(), 1);
    t.abort();
}

#[test]
fn storage_grows_in_place_and_overflows() {
    let (d, mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let lib = ops::create(&mut t, &params(qsys, "LIB1", types::LIB)).unwrap();
    let u = ops::create(&mut t, &params(lib, "SPACE", types::USRSPC)).unwrap();
    let first = ops::grow(&mut t, u, 4).unwrap();
    assert_eq!(first.len(), 1);
    let more = ops::grow(&mut t, u, 4).unwrap();
    assert_eq!(more[0].start, first[0].start + 4, "extended in place");
    let h = ops::describe(&mut t, u).unwrap();
    assert_eq!(h.size_pages, 8);
    assert_eq!(h.extents.len(), 1);
    // Fragment the free space so every growth takes a new extent.
    let mut blockers = Vec::new();
    for _ in 0..40 {
        let r = t.alloc_run(1).unwrap();
        blockers.push(r);
        ops::grow(&mut t, u, 1).unwrap();
    }
    let h = ops::describe(&mut t, u).unwrap();
    assert_eq!(h.size_pages, 48);
    assert_eq!(h.extents.len(), 16);
    assert!(h.extent_overflow.is_some());
    assert!(ops::all_extents(&mut t, &h).unwrap().len() > 16);
    t.commit().unwrap();
    let free_before = store.asp().total_free();
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let h = ops::describe(&mut t, u).unwrap();
    assert_eq!(
        ops::all_extents(&mut t, &h)
            .unwrap()
            .iter()
            .map(|r| r.len)
            .sum::<u64>(),
        48
    );
    ops::delete(&mut t, JobHandle(1), u).unwrap();
    t.commit().unwrap();
    store.checkpoint().unwrap();
    assert!(
        store.asp().total_free() > free_before + 48,
        "payload, overflow and header freed"
    );
}

#[test]
fn many_objects_and_a_reopen() {
    let (d, mut store, qsys) = open_new();
    let lib = {
        let mut t = store.begin().unwrap();
        let l = ops::create(&mut t, &params(qsys, "BIG", types::LIB)).unwrap();
        t.commit().unwrap();
        l
    };
    for batch in 0..20 {
        let mut t = store.begin().unwrap();
        for i in 0..100 {
            ops::create(
                &mut t,
                &params(lib, &format!("D{}", batch * 100 + i), types::DTAARA),
            )
            .unwrap();
        }
        t.commit().unwrap();
    }
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let all = ops::list(&mut t, lib, Some(types::DTAARA)).unwrap();
    assert_eq!(all.len(), 2000);
    assert!(all.windows(2).all(|w| w[0].0 < w[1].0), "in key order");
    assert_eq!(ops::describe(&mut t, lib).unwrap().name, name("BIG"));
    t.abort();
}
