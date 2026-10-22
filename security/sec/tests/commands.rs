// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority commands, the list commands, `DLTUSRPRF` and
//! `ANZDFTPWD`, as the security officer and as a plain user.

#![allow(clippy::too_many_lines)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{NOW, create, lib, open_new};
use sec::auth::bits::Auth;
use sec::auth::cache::Session;
use sec::auth::grant;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, dltusrprf, object, usrprf};
use sec::guard::{SecGuard, as_job};
use sec::profile::index::{self, Kind};
use sec::profile::{objects, shipped};
use store::obj::header::text_of;
use store::obj::lock::JobHandle;
use store::obj::types;

fn sysval(n: &str) -> String {
    match n {
        "QSECURITY" => "40".into(),
        "QPWDLVL" => "0".into(),
        _ => String::new(),
    }
}

fn kw(pairs: &[(&str, &str)]) -> Keywords {
    Keywords::new(pairs)
}

#[test]
fn grant_revoke_owner_group_and_lists() {
    let (mut store, qsys) = open_new();
    store.set_guard(Some(SecGuard::new(Some(qsys))));
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    let data = create(&mut t, app, "DATA", types::DTAARA, "QSECOFR");
    shipped::install(&mut t, qsys, 40, 0, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &kw(&[("USRPRF", "STAFF"), ("PASSWORD", "*NONE")]),
    )
    .unwrap();
    for u in ["ANNE", "BOB"] {
        usrprf::crtusrprf(
            &mut t,
            &env,
            &kw(&[("USRPRF", u), ("PASSWORD", "LETMEIN1")]),
        )
        .unwrap();
    }
    grant::set_public(&mut t, data, Some(Auth::EXCLUDE)).unwrap();

    // GRTOBJAUT merges unless REPLACE(*YES); RVKOBJAUT takes bits away.
    object::grtobjaut(
        &mut t,
        &env,
        data,
        &kw(&[("USER", "ANNE BOB"), ("AUT", "*USE")]),
    )
    .unwrap();
    object::grtobjaut(
        &mut t,
        &env,
        data,
        &kw(&[("USER", "ANNE"), ("AUT", "*ADD")]),
    )
    .unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(
        grant::private(&mut t, &h, "ANNE").unwrap(),
        Some(Auth::USE.union(Auth::parse(&["*ADD"], false).unwrap()))
    );
    object::grtobjaut(
        &mut t,
        &env,
        data,
        &kw(&[("USER", "ANNE"), ("AUT", "*CHANGE"), ("REPLACE", "*YES")]),
    )
    .unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(
        grant::private(&mut t, &h, "ANNE").unwrap(),
        Some(Auth::CHANGE)
    );
    object::rvkobjaut(&mut t, &env, data, &kw(&[("USER", "BOB"), ("AUT", "*USE")])).unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(
        grant::private(&mut t, &h, "BOB").unwrap(),
        None,
        "nothing left, no entry"
    );
    assert!(
        index::list(&mut t, qsys, "BOB", Kind::Private)
            .unwrap()
            .is_empty()
    );
    let shown = object::dspobjaut(&mut t, &env, data).unwrap();
    assert!(
        shown.contains(" ANNE ") && shown.contains("*CHANGE"),
        "{shown}"
    );
    assert!(
        shown.contains("*PUBLIC") && shown.contains("*EXCLUDE"),
        "{shown}"
    );

    // A plain user with *CHANGE cannot grant (no *OBJMGT) ...
    let anne = Rc::new(RefCell::new(Session::new(
        Principal::load(&mut t, qsys, "ANNE", 40).unwrap(),
    )));
    let anne_p = anne.borrow().principal.clone();
    let anne_env = Env {
        qsys,
        actor: &anne_p,
        now: NOW,
        sysval: &sysval,
    };
    let e = as_job(&anne, || {
        object::grtobjaut(
            &mut t,
            &anne_env,
            data,
            &kw(&[("USER", "BOB"), ("AUT", "*USE")]),
        )
    })
    .unwrap_err();
    assert!(
        e.to_string().contains("CPF2189") || e.msgid() == Some("CPF2189"),
        "{e}"
    );
    // ... and with *OBJMGT only what she has.
    object::grtobjaut(
        &mut t,
        &env,
        data,
        &kw(&[
            ("USER", "ANNE"),
            ("AUT", "*CHANGE *OBJMGT"),
            ("REPLACE", "*YES"),
        ]),
    )
    .unwrap();
    let anne = Rc::new(RefCell::new(Session::new(
        Principal::load(&mut t, qsys, "ANNE", 40).unwrap(),
    )));
    as_job(&anne, || {
        object::grtobjaut(
            &mut t,
            &anne_env,
            data,
            &kw(&[("USER", "BOB"), ("AUT", "*USE")]),
        )
    })
    .unwrap();
    let e = as_job(&anne, || {
        object::grtobjaut(
            &mut t,
            &anne_env,
            data,
            &kw(&[("USER", "BOB"), ("AUT", "*OBJEXIST")]),
        )
    })
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2227"), "{e}");
    // She sees only her own row.
    let shown = as_job(&anne, || object::dspobjaut(&mut t, &anne_env, data)).unwrap();
    assert!(shown.contains(" ANNE "), "{shown}");

    // CHKOBJ.
    let ok = object::chkobj(&mut t, &anne_env, data, &kw(&[("AUT", "*CHANGE")]));
    assert!(ok.is_ok());
    let e = object::chkobj(&mut t, &anne_env, data, &kw(&[("AUT", "*ALL")])).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF9802"));

    // CHGOBJOWN: the old owner keeps nothing with *REVOKE.
    object::chgobjown(&mut t, &env, data, &kw(&[("NEWOWN", "BOB")])).unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(text_of(&h.owner), "BOB");
    assert_eq!(grant::private(&mut t, &h, "QSECOFR").unwrap(), None);
    assert!(
        index::list(&mut t, qsys, "BOB", Kind::Owned)
            .unwrap()
            .iter()
            .any(|(i, _)| *i == data)
    );

    // CHGOBJPGP.
    object::chgobjpgp(&mut t, &env, data, &kw(&[("NEWPGP", "STAFF")])).unwrap();
    assert_eq!(
        text_of(&grant::header(&mut t, data).unwrap().primary_group),
        "STAFF"
    );
    let listed = object::objects_of(&mut t, &env, "STAFF", Kind::Group).unwrap();
    assert!(
        listed.contains("DATA") && listed.contains("APPLIB"),
        "{listed}"
    );

    // GRTUSRAUT copies ANNE's private authorities to STAFF... STAFF is
    // the group already, so copy them to a new user instead.
    usrprf::crtusrprf(&mut t, &env, &kw(&[("USRPRF", "CAROL")])).unwrap();
    object::grtusraut(&mut t, &env, &kw(&[("USER", "CAROL"), ("REFUSER", "ANNE")])).unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(
        grant::private(&mut t, &h, "CAROL").unwrap(),
        grant::private(&mut t, &h, "ANNE").unwrap()
    );

    // Authorization lists.
    object::crtautl(
        &mut t,
        &env,
        &kw(&[("AUTL", "PAYROLL"), ("TEXT", "'Payroll'")]),
    )
    .unwrap();
    object::setautle(
        &mut t,
        &env,
        &kw(&[("AUTL", "PAYROLL"), ("USER", "ANNE"), ("AUT", "*USE")]),
        true,
    )
    .unwrap();
    let e = object::setautle(
        &mut t,
        &env,
        &kw(&[("AUTL", "PAYROLL"), ("USER", "ANNE")]),
        true,
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2272"));
    object::setautle(
        &mut t,
        &env,
        &kw(&[("AUTL", "PAYROLL"), ("USER", "ANNE"), ("AUT", "*ALL")]),
        false,
    )
    .unwrap();
    let shown = object::dspautl(&mut t, &env, "PAYROLL").unwrap();
    assert!(
        shown.contains("Payroll")
            && shown
                .lines()
                .any(|l| l.starts_with(" ANNE") && l.ends_with('X')),
        "{shown}"
    );
    object::grtobjaut(&mut t, &env, data, &kw(&[("AUTL", "PAYROLL")])).unwrap();
    let shown = object::dspautlobj(&mut t, &env, "PAYROLL").unwrap();
    assert!(shown.contains("DATA"), "{shown}");
    object::rvkobjaut(&mut t, &env, data, &kw(&[("AUTL", "PAYROLL")])).unwrap();
    assert!(
        !object::dspautlobj(&mut t, &env, "PAYROLL")
            .unwrap()
            .contains("DATA")
    );
    object::rmvautle(&mut t, &env, &kw(&[("AUTL", "PAYROLL"), ("USER", "ANNE")])).unwrap();
    assert!(object::rmvautle(&mut t, &env, &kw(&[("AUTL", "PAYROLL"), ("USER", "ANNE")])).is_err());

    // DLTUSRPRF: BOB owns DATA; *NODLT stops it, *CHGOWN moves it.
    let job = JobHandle(1);
    let e = dltusrprf::dltusrprf(&mut t, &env, job, &kw(&[("USRPRF", "BOB")])).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2204"));
    dltusrprf::dltusrprf(
        &mut t,
        &env,
        job,
        &kw(&[("USRPRF", "BOB"), ("OWNOBJOPT", "*CHGOWN QSECOFR")]),
    )
    .unwrap();
    assert!(!objects::exists(&mut t, qsys, "BOB"));
    assert_eq!(
        text_of(&grant::header(&mut t, data).unwrap().owner),
        "QSECOFR"
    );
    // STAFF is DATA's primary group.
    let e = dltusrprf::dltusrprf(&mut t, &env, job, &kw(&[("USRPRF", "STAFF")])).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2204"));
    dltusrprf::dltusrprf(
        &mut t,
        &env,
        job,
        &kw(&[("USRPRF", "STAFF"), ("PGPOPT", "*CHGPGP *NONE")]),
    )
    .unwrap();
    assert!(text_of(&grant::header(&mut t, data).unwrap().primary_group).is_empty());
    // CAROL's private authority goes with her.
    dltusrprf::dltusrprf(&mut t, &env, job, &kw(&[("USRPRF", "CAROL")])).unwrap();
    let h = grant::header(&mut t, data).unwrap();
    assert_eq!(grant::private(&mut t, &h, "CAROL").unwrap(), None);
    assert!(dltusrprf::dltusrprf(&mut t, &env, job, &kw(&[("USRPRF", "QSECOFR")])).is_err());
    assert!(index::verify(&mut t, qsys).unwrap().is_empty());
}

#[test]
fn default_passwords() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, 40, 0, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(
        &mut t,
        &env,
        &kw(&[("USRPRF", "DAVE"), ("PASSWORD", "DAVE")]),
    )
    .unwrap();
    usrprf::crtusrprf(
        &mut t,
        &env,
        &kw(&[("USRPRF", "ERIN"), ("PASSWORD", "OTHER1")]),
    )
    .unwrap();
    let r = dltusrprf::anzdftpwd(&mut t, &env, &kw(&[("ACTION", "*DISABLE *PWDEXP")])).unwrap();
    assert!(r.contains(" DAVE ") && !r.contains(" ERIN "), "{r}");
    let (_, dave) = objects::read(&mut t, qsys, "DAVE").unwrap();
    assert!(dave.disabled && dave.pwdexp);
    let (_, erin) = objects::read(&mut t, qsys, "ERIN").unwrap();
    assert!(!erin.disabled);
}
