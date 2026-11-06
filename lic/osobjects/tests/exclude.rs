// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*PUBLIC *EXCLUDE` on a command, a program and a library, after the
//! first IPL's order (the objects, then the shipped profiles re-owning
//! them) and a second IPL: a plain profile is refused each through the
//! paths QAP's requests take (the analyzer's command source, a program
//! load, a name in a library), the security officer is not.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use cmd::analyze::CmdSource;
use qsname::{LibSpec, Name};
use sec::auth::bits::Auth;
use sec::auth::cache::Session;
use sec::auth::principal::Principal;
use sec::guard::{SecGuard, as_job};
use store::obj::ops::NameContext;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig, StoreError};

const NOW: u64 = 1_767_225_600;

fn create(t: &mut Txn<'_>, library: ObjectId, name: &str, type_code: u16) -> ObjectId {
    ops::create(
        t,
        &CreateParams {
            library,
            name: Name::parse(name).unwrap(),
            type_code,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap()
}

/// A store after `ipls` IPLs (the first, then more), `QAPTEST`, and
/// `QGPL/QAPXPGM` and `QAPXLIB` (with `QAPXDTA`) excluded.
fn system(ipls: usize) -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(131_072, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 2048,
        now: NOW,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    for lib in ["QGPL", "QUSRSYS", "QSYS2", "QRECOVERY", "QSPL"] {
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
    let stamp = wm::obj::Stamp { owner: "QSYS".into(), system: "QS400".into(), now: NOW };
    // The first IPL: the objects, then the profiles (re-owning them).
    osobjects::create(&mut t, qsys, &stamp).unwrap();
    sec::profile::shipped::install(&mut t, qsys, 40, 0, NOW).unwrap();
    for _ in 1..ipls {
        osobjects::create(&mut t, qsys, &stamp).unwrap();
        sec::profile::shipped::install(&mut t, qsys, 40, 0, NOW).unwrap();
    }
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let sysval = |n: &str| match n {
        "QSECURITY" => "40".to_string(),
        "QPWDLVL" => "0".to_string(),
        _ => String::new(),
    };
    let env = sec::cmd::Env { qsys, actor: &secofr, now: NOW, sysval: &sysval };
    let k = sec::cmd::Keywords::new(&[("USRPRF", "QAPTEST"), ("PASSWORD", "QAP4STEP")]);
    sec::cmd::usrprf::crtusrprf(&mut t, &env, &k).unwrap();
    let qgpl = ops::find_library(&mut t, qsys, Name::parse("QGPL").unwrap()).unwrap().id;
    let pgm = create(&mut t, qgpl, "QAPXPGM", types::PGM);
    sec::auth::grant::set_public(&mut t, pgm, Some(Auth::EXCLUDE)).unwrap();
    let lib = create(&mut t, qsys, "QAPXLIB", types::LIB);
    create(&mut t, lib, "QAPXDTA", types::DTAARA);
    sec::auth::grant::set_public(&mut t, lib, Some(Auth::EXCLUDE)).unwrap();
    t.commit().unwrap();
    store.set_guard(Some(SecGuard::new(Some(qsys))));
    (store, qsys)
}

/// What `user` gets for the command, the program and the library: the
/// message of each refusal, `None` where it was allowed.
fn outcomes(store: &mut Store, qsys: ObjectId, user: &str) -> [Option<String>; 3] {
    let mut t = store.begin().unwrap();
    let me = Principal::load(&mut t, qsys, user, 40).unwrap();
    let s = Rc::new(RefCell::new(Session::new(me)));
    let qgpl = ops::find_library(&mut t, qsys, Name::parse("QGPL").unwrap()).unwrap().id;
    let names = NameContext { curlib: Some(qgpl), libl: vec![qsys, qgpl] };
    as_job(&s, || {
        let cmd = {
            let mut src = cmd::store::StoreSource { txn: &mut t, qsys, names: names.clone() };
            match src.lookup("DSPPTF", None) {
                Ok(Some(_)) => None,
                Ok(None) => Some("not found".to_string()),
                Err(e) => Some(e.msgid().to_string()),
            }
        };
        let not_authorized = |e: &StoreError| match e {
            StoreError::NotAuthorized { msgid, .. } => Some((*msgid).to_string()),
            other => panic!("{other}"),
        };
        let found = ops::resolve(&mut t, qsys, &names, LibSpec::Libl, Name::parse("QAPXPGM").unwrap(), types::PGM).unwrap();
        let pgm = match ops::describe(&mut t, found.id) {
            Ok(_) => None,
            Err(e) => not_authorized(&e),
        };
        let lib = LibSpec::Name(Name::parse("QAPXLIB").unwrap());
        let lib = match ops::resolve(&mut t, qsys, &names, lib, Name::parse("QAPXDTA").unwrap(), types::DTAARA) {
            Ok(_) => None,
            Err(e) => not_authorized(&e),
        };
        [cmd, pgm, lib]
    })
}

#[test]
fn excluded_objects_refuse_a_plain_profile() {
    for ipls in [1, 2] {
        let (mut store, qsys) = system(ipls);
        let [cmd, pgm, lib] = outcomes(&mut store, qsys, "QAPTEST");
        assert!(cmd.as_deref().is_some_and(|m| m != "not found"), "after {ipls} IPL(s), DSPPTF: {cmd:?}");
        assert!(pgm.is_some(), "after {ipls} IPL(s), QGPL/QAPXPGM was allowed");
        assert!(lib.is_some(), "after {ipls} IPL(s), QAPXLIB was allowed");
        assert_eq!(outcomes(&mut store, qsys, "QSECOFR"), [None, None, None]);
    }
}

/// The first IPL's re-owning once gave every command `*PUBLIC *USE`.
#[test]
fn every_shipped_exclusion_survives_the_ipls() {
    for ipls in [1, 2] {
        let (mut store, qsys) = system(ipls);
        let mut t = store.begin().unwrap();
        let lib = store::obj::library::Library::load(&mut t, qsys).unwrap();
        for name in sys_cpps::inventory::PUBLIC_EXCLUDE {
            let Some(id) = lib.lookup(&mut t, types::CMD, Name::parse(name).unwrap()).unwrap() else {
                continue;
            };
            let (_, h) = ops::read_header(&mut t, id).unwrap();
            assert_eq!(h.public_authority, store::obj::Authority::EXCLUDE, "after {ipls} IPL(s), {name}");
        }
    }
}
