// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A V1R1M0 system's `QSERVER` gets `NetServer`'s entries once, at an IPL
//! with the new code or across a slip install, and keeps the customer's
//! own; a slip install keeps the system's work management objects.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::{CreateParams, JobHandle, NameContext, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};
use wm::obj::jobd::JobD;
use wm::obj::outq::OutQ;
use wm::obj::sbsd::{self, Pje, Sbsd};
use wm::obj::{Stamp, find, read, shipped, upgrade, write};

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

fn id_of(t: &mut Txn<'_>, qsys: ObjectId, lib: &str, name: &str, ty: u16) -> ObjectId {
    let names = NameContext {
        curlib: None,
        libl: vec![qsys],
    };
    find(t, qsys, &names, lib, name, ty).unwrap()
}

fn qserver(t: &mut Txn<'_>, qsys: ObjectId) -> Sbsd {
    let id = id_of(t, qsys, "QSYS", "QSERVER", types::SBSD);
    Sbsd::decode(&read(t, id).unwrap()).unwrap()
}

fn set_qserver(t: &mut Txn<'_>, qsys: ObjectId, s: &Sbsd) {
    let id = id_of(t, qsys, "QSYS", "QSERVER", types::SBSD);
    write(t, id, &s.encode()).unwrap();
}

fn shipped_qserver() -> Sbsd {
    sbsd::shipped()
        .into_iter()
        .find(|(n, _)| *n == "QSERVER")
        .unwrap()
        .1
}

fn mine() -> Pje {
    Pje {
        pgm: ("MYSRV".into(), "MYLIB".into()),
        job: "MYSRV".into(),
        ..shipped_qserver().pjes[0].clone()
    }
}

/// The store as V1R1M0 left it: QSERVER without `NetServer`'s entries but
/// with one of the customer's, no `QSYS/QZLSSERVER` job description.
fn as_v1r1m0(t: &mut Txn<'_>, qsys: ObjectId) {
    let mut s = shipped_qserver();
    s.rtges.clear();
    s.jobqes.clear();
    s.pjes.retain(|p| p.pgm.0.starts_with("QPWFSERV"));
    s.pjes.push(mine());
    set_qserver(t, qsys, &s);
    let jd = id_of(t, qsys, "QSYS", "QZLSSERVER", types::JOBD);
    ops::delete(t, JobHandle(0), jd).unwrap();
}

fn has_netserver(s: &Sbsd) -> bool {
    let q = shipped_qserver();
    q.rtges.iter().all(|r| s.rtges.contains(r))
        && q.jobqes.iter().all(|e| s.jobqes.contains(e))
        && q.pjes
            .iter()
            .filter(|p| p.pgm.0.starts_with("QZLS"))
            .all(|p| s.pjes.contains(p))
}

#[test]
fn the_ipl_adds_netserver_once_and_keeps_the_customers_entries() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, &stamp()).unwrap();
    as_v1r1m0(&mut t, qsys);
    assert!(!has_netserver(&qserver(&mut t, qsys)));
    // The IPL after a PTF brought the code: the entries and the job
    // description.
    let made = shipped::install(&mut t, qsys, &stamp()).unwrap();
    assert_eq!(made, 2, "QSERVER changed and QZLSSERVER made");
    let s = qserver(&mut t, qsys);
    assert!(has_netserver(&s), "{s:?}");
    assert!(s.pjes.contains(&mine()));
    id_of(&mut t, qsys, "QSYS", "QZLSSERVER", types::JOBD);
    // The customer removes an entry: it stays removed.
    let mut s = s;
    s.pjes.retain(|p| p.pgm.0 != "QZLSFILET");
    set_qserver(&mut t, qsys, &s);
    assert_eq!(shipped::install(&mut t, qsys, &stamp()).unwrap(), 0);
    assert_eq!(qserver(&mut t, qsys), s);
    t.commit().unwrap();
}

/// What the load's restore does to the objects: the shipped payloads
/// over them, and the load's own objects (the marker) made.
fn restore_load(t: &mut Txn<'_>, qsys: ObjectId) {
    set_qserver(t, qsys, &shipped_qserver());
    let q = id_of(t, qsys, "QGPL", "QPRINT", types::OUTQ);
    write(t, q, &OutQ::new("Default printer output queue").encode()).unwrap();
    let names = NameContext {
        curlib: None,
        libl: vec![qsys],
    };
    if find(t, qsys, &names, "QSYS", "QZLSSERVER", types::JOBD).is_err() {
        wm::obj::create(
            t,
            qsys,
            Name::parse("QZLSSERVER").unwrap(),
            types::JOBD,
            "",
            &stamp(),
            &JobD::default().encode(),
        )
        .unwrap();
    }
}

#[test]
fn a_slip_install_keeps_the_systems_objects_and_adds_netserver() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, &stamp()).unwrap();
    as_v1r1m0(&mut t, qsys);
    let q = id_of(&mut t, qsys, "QGPL", "QPRINT", types::OUTQ);
    let mut outq = OutQ::decode(&read(&mut t, q).unwrap()).unwrap();
    outq.files.push(7);
    write(&mut t, q, &outq.encode()).unwrap();

    let kept = upgrade::keep(&mut t, qsys).unwrap();
    assert!(kept.len() > 60);
    restore_load(&mut t, qsys);
    let lines = upgrade::put_back(&mut t, qsys, &kept, &stamp()).unwrap();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("QSERVER: NetServer: added routing entry 800")),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l.contains("QGPL/QPRINT")), "{lines:?}");
    let s = qserver(&mut t, qsys);
    assert!(has_netserver(&s), "{s:?}");
    assert!(s.pjes.contains(&mine()));
    let q = id_of(&mut t, qsys, "QGPL", "QPRINT", types::OUTQ);
    assert_eq!(
        OutQ::decode(&read(&mut t, q).unwrap()).unwrap().files,
        vec![7]
    );
    // The IPL after it adds nothing.
    assert_eq!(shipped::install(&mut t, qsys, &stamp()).unwrap(), 0);

    // The next slip install: an entry the customer removed stays removed.
    let mut s = s;
    s.pjes.retain(|p| p.pgm.0 != "QZLSFILET");
    set_qserver(&mut t, qsys, &s);
    let kept = upgrade::keep(&mut t, qsys).unwrap();
    restore_load(&mut t, qsys);
    upgrade::put_back(&mut t, qsys, &kept, &stamp()).unwrap();
    assert_eq!(qserver(&mut t, qsys), s);

    // Objects the system has as shipped are left as the load put them.
    let (mut store2, qsys2) = open_new();
    let mut t2 = store2.begin().unwrap();
    shipped::install(&mut t2, qsys2, &stamp()).unwrap();
    let kept = upgrade::keep(&mut t2, qsys2).unwrap();
    restore_load(&mut t2, qsys2);
    assert!(
        upgrade::put_back(&mut t2, qsys2, &kept, &stamp())
            .unwrap()
            .is_empty()
    );
    t2.commit().unwrap();

    // V1R1M0's QSERVER as shipped: the load's copy is the result, and the
    // additions are still told.
    let (mut store3, qsys3) = open_new();
    let mut t3 = store3.begin().unwrap();
    shipped::install(&mut t3, qsys3, &stamp()).unwrap();
    as_v1r1m0(&mut t3, qsys3);
    let mut s = qserver(&mut t3, qsys3);
    s.pjes.retain(|p| p.pgm.0 != "MYSRV");
    set_qserver(&mut t3, qsys3, &s);
    let kept = upgrade::keep(&mut t3, qsys3).unwrap();
    restore_load(&mut t3, qsys3);
    let lines = upgrade::put_back(&mut t3, qsys3, &kept, &stamp()).unwrap();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("QSERVER: NetServer: added")),
        "{lines:?}"
    );
    assert_eq!(qserver(&mut t3, qsys3), shipped_qserver());
    t3.commit().unwrap();
    t.commit().unwrap();
}
