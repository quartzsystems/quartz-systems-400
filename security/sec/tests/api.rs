// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security APIs' questions answered from a store.

mod common;

use common::{NOW, create, lib, open_new};
use sec::api::{Answer, Query, answer};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::auth::{autl, grant};
use sec::cmd::{Env, Keywords, usrprf};
use sec::profile::{UsrPrf, shipped};
use store::obj::header::Authority;
use store::obj::types;

fn sysval(n: &str) -> String {
    if n == "QSECURITY" {
        "40".into()
    } else {
        String::new()
    }
}

fn obj(name: &str) -> (String, String, String) {
    (name.into(), "APPLIB".into(), "*DTAARA".into())
}

#[test]
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
fn queries_answer_from_the_store() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    let data = create(&mut t, app, "DATA", types::DTAARA, "QSECOFR");
    create(&mut t, app, "OTHER", types::DTAARA, "QSECOFR");
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "CLERKS"), ("PASSWORD", "*NONE")])).unwrap();
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "ANNE"), ("PASSWORD", "SECRET77"), ("GRPPRF", "CLERKS"), ("TEXT", "'Anne'")]),
    )
    .unwrap();
    grant::set_private(&mut t, qsys, data, "ANNE", Some(Auth::CHANGE)).unwrap();
    grant::set_public(&mut t, data, Some(Auth::EXCLUDE)).unwrap();
    let anne = Principal::load(&mut t, qsys, "ANNE", 40).unwrap();
    let ask = |t: &mut store::txn::Txn<'_>, me: &Principal, q: Query| answer(t, qsys, me, &q);

    // QSYRUSRI: one's own profile, another's only with *READ to it.
    let Some(Answer::Profile { data: d }) = ask(&mut t, &anne, Query::Profile { user: "*CURRENT".into() }).unwrap() else {
        panic!("not a profile");
    };
    let p = UsrPrf::decode(&d).unwrap();
    assert_eq!(p.name, "ANNE");
    assert_eq!(p.grpprf, "CLERKS");
    let e = ask(&mut t, &anne, Query::Profile { user: "QSECOFR".into() }).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2217"), "{e}");
    assert!(ask(&mut t, &secofr, Query::Profile { user: "ANNE".into() }).is_ok());

    // QSYCUSRS.
    let special = |t: &mut store::txn::Txn<'_>, user: &str| {
        let q = Query::Special {
            user: user.into(),
            auths: vec!["*ALLOBJ".into()],
        };
        matches!(answer(t, qsys, &anne, &q).unwrap(), Some(Answer::Yes { yes: true }))
    };
    assert!(special(&mut t, "QSECOFR"));
    assert!(!special(&mut t, "ANNE"));

    // QSYCUSRA: ANNE has a private *CHANGE, CLERKS nothing.
    let check = |t: &mut store::txn::Txn<'_>, user: &str, auths: &[&str]| {
        let q = Query::Check {
            user: user.into(),
            object: obj("DATA"),
            auths: auths.iter().map(ToString::to_string).collect(),
        };
        matches!(answer(t, qsys, &secofr, &q).unwrap(), Some(Answer::Yes { yes: true }))
    };
    assert!(check(&mut t, "ANNE", &["*CHANGE"]));
    assert!(!check(&mut t, "ANNE", &["*OBJEXIST"]));
    assert!(!check(&mut t, "CLERKS", &["*USE"]));
    assert!(check(&mut t, "QSECOFR", &["*ALL"]));

    // QSYRUSRA: the source of ANNE's authority.
    let Some(Answer::Auth { info }) = ask(&mut t, &secofr, Query::UserAuth { user: "ANNE".into(), object: obj("DATA") }).unwrap() else {
        panic!("no authority");
    };
    assert_eq!(info.bits & Auth::MASK, Auth::CHANGE.0);
    assert_eq!(info.owner, "QSECOFR");
    assert!(!info.source.is_empty());

    // QSYLOBJA: ANNE's private authority; QSECOFR owns DATA.
    let Some(Answer::Objects { rows }) = ask(&mut t, &anne, Query::Objects { user: "*CURRENT".into(), kind: 0 }).unwrap() else {
        panic!("no objects");
    };
    assert!(rows.iter().any(|r| r.name == "DATA" && r.lib == "APPLIB" && !r.owned));
    let Some(Answer::Objects { rows }) = ask(&mut t, &secofr, Query::Objects { user: "*CURRENT".into(), kind: 0 }).unwrap() else {
        panic!("no objects");
    };
    assert!(rows.iter().any(|r| r.name == "DATA" && r.owned));
    assert!(ask(&mut t, &anne, Query::Objects { user: "QSECOFR".into(), kind: 0 }).is_err());

    // QSYRAUTU: the owner first, *PUBLIC last.
    let Some(Answer::Users { rows }) = ask(&mut t, &anne, Query::Holders { object: obj("DATA") }).unwrap() else {
        panic!("no holders");
    };
    assert_eq!(rows.first().unwrap().name, "QSECOFR");
    assert_eq!(rows.last().unwrap().name, "*PUBLIC");
    assert_eq!(rows.last().unwrap().bits & Auth::MASK, Auth::EXCLUDE.0);
    assert!(rows.iter().any(|r| r.name == "ANNE" && r.bits & Auth::MASK == Auth::CHANGE.0));

    // QSYLAUTU: groups and their members.
    let users = |t: &mut store::txn::Txn<'_>, select: &str| -> Vec<String> {
        let q = Query::Users {
            select: select.into(),
            group: "CLERKS".into(),
        };
        let Some(Answer::Users { rows }) = answer(t, qsys, &anne, &q).unwrap() else {
            panic!("no users");
        };
        rows.into_iter().map(|r| r.name).collect()
    };
    assert_eq!(users(&mut t, "*MEMBER"), ["ANNE"]);
    assert!(users(&mut t, "*GROUP").contains(&"CLERKS".to_string()));
    assert!(!users(&mut t, "*USRPRF").contains(&"CLERKS".to_string()));

    // QSYLATLO.
    autl::create(&mut t, qsys, "PAYROLL", "", "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
    grant::set_autl(&mut t, qsys, data, Some("PAYROLL")).unwrap();
    let Some(Answer::Objects { rows }) = ask(&mut t, &anne, Query::ListObjects { list: "PAYROLL".into() }).unwrap() else {
        panic!("no objects");
    };
    assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["DATA"]);

    // QSYRUPWD and QSYSUPWD: ANNE's data moved onto CLERKS.
    assert!(ask(&mut t, &anne, Query::GetPassword { user: "ANNE".into() }).is_err());
    let Some(Answer::Bytes { data: blob }) = ask(&mut t, &secofr, Query::GetPassword { user: "ANNE".into() }).unwrap() else {
        panic!("no data");
    };
    assert!(blob.starts_with(sec::api::PWD_MAGIC));
    ask(&mut t, &secofr, Query::SetPassword { user: "CLERKS".into(), data: blob }).unwrap();
    let (_, clerks) = sec::profile::objects::read(&mut t, qsys, "CLERKS").unwrap();
    let (_, anne_p) = sec::profile::objects::read(&mut t, qsys, "ANNE").unwrap();
    assert!(!clerks.pwd_none);
    assert_eq!(clerks.hashes, anne_p.hashes);

    // The service's own questions are not answered here.
    let q = Query::ReleaseHandle { handle: vec![1; 12] };
    assert!(ask(&mut t, &anne, q).unwrap().is_none());
}
