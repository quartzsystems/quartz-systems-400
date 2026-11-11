// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operating system's load installed into a fresh store, then again
//! over itself as a slip install: system objects replaced, user objects
//! untouched, signatures checked.

use std::path::Path;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use instmedia::osload::{self, Request};
use licpgm::{LoadType, ProductStore, SystemInfo};
use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

thread_local! {
    static DEV: std::cell::RefCell<Option<Arc<MemBlockDev>>> = const { std::cell::RefCell::new(None) };
}

fn manifest(id: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../licpgm/products/{id}.toml"));
    std::fs::read_to_string(p).unwrap()
}

fn lib(t: &mut store::Txn<'_>, parent: ObjectId, name: &str) -> ObjectId {
    ops::create(
        t,
        &CreateParams {
            library: parent,
            name: Name::parse(name).unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap()
}

fn system_on(d: Arc<MemBlockDev>) -> (Store, ObjectId) {
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 1024,
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
    for l in ["QUSRSYS", "QSYS2", "QGPL", "QHLPSYS", "QSPL"] {
        lib(&mut t, qsys, l);
    }
    t.commit().unwrap();
    (store, qsys)
}

fn count(store: &mut Store, qsys: ObjectId, lib_name: &str) -> usize {
    let mut t = store.begin().unwrap();
    let l = ops::find_library(&mut t, qsys, Name::parse(lib_name).unwrap()).unwrap();
    let n = ops::list(&mut t, l.id, None).unwrap().len();
    t.abort();
    n
}

#[test]
#[allow(clippy::too_many_lines)] // one scenario
fn the_os_installs_fresh_and_again_as_a_slip() {
    let key = SigningKey::from_bytes(&[42; 32]);
    let public = key.verifying_key().to_bytes();
    let built = lpbuild::build(&manifest("QS00SS1"), &key, NOW).unwrap();
    let dev = Arc::new(MemBlockDev::with_seed(65536, 9));
    DEV.with(|d| *d.borrow_mut() = Some(dev.clone()));
    let (mut store, qsys) = system_on(dev);
    let before = count(&mut store, qsys, "QSYS");
    let req = Request {
        stream: built.stream.clone(),
        product: "QS00SS1",
        options: Vec::new(),
        key: Some(public),
        system: SystemInfo::current("QS40001", "P05"),
        accepted_by: "QSECOFR",
        now: NOW,
    };
    let r = osload::install(&mut store, qsys, &req).unwrap();
    assert!(r.objects > 300, "{}", osload::summary(&r));
    // The store reopens clean after the install.
    store.close().unwrap();
    let (mut store, _) = Store::open(
        vec![Box::new(DEV.with(|d| d.borrow().clone().unwrap())) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap();
    assert_eq!(r.replaced, 0);
    assert!(r.verified >= r.objects, "{}", osload::summary(&r));
    assert_eq!(r.bad_signatures, 0);
    assert!(
        r.loads
            .iter()
            .any(|k| k.option.is_base() && k.load_type == LoadType::Code)
    );
    assert!(count(&mut store, qsys, "QSYS") > before + 300);

    // A user library with an object, then the slip install.
    {
        let mut t = store.begin().unwrap();
        let mine = lib(&mut t, qsys, "MYLIB");
        ops::create(
            &mut t,
            &CreateParams {
                library: mine,
                name: Name::parse("KEEPME").unwrap(),
                type_code: types::DTAARA,
                attribute: String::new(),
                text: "user data".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
    }
    let qsys_objects = count(&mut store, qsys, "QSYS");
    let kind = osload::install_kind(&store, release::RELEASE).unwrap();
    assert_eq!(kind, release::InstallKind::Slip);
    let again = osload::install(&mut store, qsys, &req).unwrap();
    assert_eq!(again.objects, r.objects);
    assert_eq!(again.replaced, r.objects, "{}", osload::summary(&again));
    assert_eq!(again.removed, 0);
    store.close().unwrap();
    let (mut store, _) = Store::open(
        vec![Box::new(DEV.with(|d| d.borrow().clone().unwrap())) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap();
    assert_eq!(count(&mut store, qsys, "QSYS"), qsys_objects);
    assert_eq!(count(&mut store, qsys, "MYLIB"), 1);
    // The product is recorded installed once.
    let cell = std::cell::RefCell::new(&mut store);
    let p = lpstore::Products::new(&cell, qsys, lpstore::stamp(NOW));
    let base: Vec<_> = p
        .loads()
        .into_iter()
        .filter(|(_, l)| {
            l.key.product.as_str() == "QS00SS1"
                && l.key.option.is_base()
                && l.key.load_type == LoadType::Code
        })
        .collect();
    assert_eq!(base.len(), 1);
    assert!(base[0].1.state.is_installed());
    drop(p);
    // The retranslation pass visits programs in user libraries only.
    let mut seen = 0;
    let n = osload::retranslate_user_programs(
        &mut store,
        qsys,
        release::InstallKind::Slip,
        &mut |_, _, all| {
            assert!(!all);
            seen += 1;
            Ok(0)
        },
    )
    .unwrap();
    assert_eq!((n, seen), (0, 0));

    // A product signed with a key the system does not know: nothing
    // verified, nothing refused.
    let other = SigningKey::from_bytes(&[43; 32]);
    let st1 = lpbuild::build(&manifest("QS00ST1"), &other, NOW).unwrap();
    let r = osload::install(
        &mut store,
        qsys,
        &Request {
            stream: st1.stream,
            product: "QS00ST1",
            ..req
        },
    )
    .unwrap();
    assert_eq!((r.verified, r.bad_signatures), (0, 0));
}

#[test]
fn the_http_server_installs_its_library() {
    let key = SigningKey::from_bytes(&[42; 32]);
    let public = key.verifying_key().to_bytes();
    let built = lpbuild::build(&manifest("QS00DG1"), &key, NOW).unwrap();
    let (mut store, qsys) = system_on(Arc::new(MemBlockDev::with_seed(65536, 10)));
    // The catalog seeded (its prerequisites installed), then the load
    // restored over the seeded one.
    lpstore::seed(&mut store, qsys, NOW).unwrap();
    let r = osload::install(
        &mut store,
        qsys,
        &Request {
            stream: built.stream,
            product: "QS00DG1",
            options: Vec::new(),
            key: Some(public),
            system: SystemInfo::current("QS40001", "P05"),
            accepted_by: "QSECOFR",
            now: NOW,
        },
    )
    .unwrap();
    assert_eq!(r.bad_signatures, 0);
    assert!(r.verified >= 1, "{}", osload::summary(&r));
    assert_eq!(count(&mut store, qsys, "QHTTPSVR"), 1);
    let cell = std::cell::RefCell::new(&mut store);
    let mut p = lpstore::Products::new(&cell, qsys, lpstore::stamp(NOW));
    let objs = lpstore::Products::new(&cell, qsys, lpstore::stamp(NOW));
    let pid = "QS00DG1".parse().unwrap();
    let report = licpgm::directory::check::chkprdopt(&mut p, &objs, &pid, None, None).unwrap();
    assert!(report.clean(), "{report:?}");
}

/// QSERVER as V1R1M0 had it, with an entry of the customer's, and no
/// `QSYS/QZLSSERVER`; a spooled file on `QGPL/QPRINT`.
fn as_v1r1m0(t: &mut store::Txn<'_>, qsys: ObjectId) -> wm::obj::sbsd::Pje {
    use wm::obj::sbsd::{Pje, Sbsd};
    let id = |t: &mut store::Txn<'_>, lib: &str, n: &str, ty: u16| {
        let l = if lib == "QSYS" {
            qsys
        } else {
            ops::find_library(t, qsys, Name::parse(lib).unwrap())
                .unwrap()
                .id
        };
        ops::list(t, l, Some(ty))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == n)
            .unwrap()
            .1
    };
    let q = id(t, "QSYS", "QSERVER", types::SBSD);
    let mut s = Sbsd::decode(&wm::obj::read(t, q).unwrap()).unwrap();
    s.rtges.clear();
    s.jobqes.clear();
    s.pjes.retain(|p| p.pgm.0.starts_with("QPWFSERV"));
    let mine = Pje {
        pgm: ("MYSRV".into(), "MYLIB".into()),
        job: "MYSRV".into(),
        ..s.pjes[0].clone()
    };
    s.pjes.push(mine.clone());
    wm::obj::write(t, q, &s.encode()).unwrap();
    let jd = id(t, "QSYS", "QZLSSERVER", types::JOBD);
    ops::delete(t, store::obj::JobHandle(0), jd).unwrap();
    let o = id(t, "QGPL", "QPRINT", types::OUTQ);
    let mut outq = wm::obj::outq::OutQ::decode(&wm::obj::read(t, o).unwrap()).unwrap();
    outq.files.push(7);
    wm::obj::write(t, o, &outq.encode()).unwrap();
    mine
}

#[test]
fn a_slip_install_keeps_qserver_and_adds_netserver() {
    use wm::obj::sbsd::Sbsd;
    let key = SigningKey::from_bytes(&[42; 32]);
    let built = lpbuild::build(&manifest("QS00SS1"), &key, NOW).unwrap();
    let (mut store, qsys) = system_on(Arc::new(MemBlockDev::with_seed(65536, 9)));
    let req = Request {
        stream: built.stream.clone(),
        product: "QS00SS1",
        options: Vec::new(),
        key: Some(key.verifying_key().to_bytes()),
        system: SystemInfo::current("QS40001", "P05"),
        accepted_by: "QSECOFR",
        now: NOW,
    };
    osload::install(&mut store, qsys, &req).unwrap();
    let mut t = store.begin().unwrap();
    let mine = as_v1r1m0(&mut t, qsys);
    let kept = wm::obj::upgrade::keep(&mut t, qsys).unwrap();
    t.commit().unwrap();
    osload::install(&mut store, qsys, &req).unwrap();
    let stamp = wm::obj::Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
    };
    let mut t = store.begin().unwrap();
    // The restore alone puts the load's copies over the system's.
    {
        let q = ops::list(&mut t, qsys, Some(types::SBSD))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == "QSERVER")
            .unwrap()
            .1;
        let s = Sbsd::decode(&wm::obj::read(&mut t, q).unwrap()).unwrap();
        assert!(!s.pjes.contains(&mine));
    }
    let lines = wm::obj::upgrade::put_back(&mut t, qsys, &kept, &stamp).unwrap();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("QSERVER: NetServer: added routing entry 800")),
        "{lines:?}"
    );
    let find = |t: &mut store::Txn<'_>, l: ObjectId, n: &str, ty: u16| {
        ops::list(t, l, Some(ty))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == n)
            .map(|(_, id)| id)
    };
    let q = find(&mut t, qsys, "QSERVER", types::SBSD).unwrap();
    let s = Sbsd::decode(&wm::obj::read(&mut t, q).unwrap()).unwrap();
    assert!(s.pjes.contains(&mine), "the customer's entry is kept");
    assert!(
        s.rtges
            .iter()
            .any(|r| r.seq == 800 && r.pgm.0 == "QZLSSERVER")
    );
    assert!(s.pjes.iter().any(|p| p.pgm.0 == "QZLSFILE"));
    assert!(s.jobqes.iter().any(|e| e.jobq.0 == "QPWFSERVER"));
    assert!(find(&mut t, qsys, "QZLSSERVER", types::JOBD).is_some());
    let qgpl = ops::find_library(&mut t, qsys, Name::parse("QGPL").unwrap())
        .unwrap()
        .id;
    let o = find(&mut t, qgpl, "QPRINT", types::OUTQ).unwrap();
    let outq = wm::obj::outq::OutQ::decode(&wm::obj::read(&mut t, o).unwrap()).unwrap();
    assert_eq!(outq.files, vec![7], "the queue's spooled files are kept");
    t.commit().unwrap();
}
