// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The guard on a store: the store's own operations are checked for a
//! job's session, new objects get their owner and `*PUBLIC`, the
//! indexes follow creation and deletion, decisions are cached.

#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{NOW, create, lib, open_new};
use qsname::{LibSpec, Name};
use sec::auth::bits::Auth;
use sec::auth::cache::Session;
use sec::auth::grant;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::guard::{SecGuard, as_job, as_system, take_failures};
use sec::profile::index::{self, Kind};
use sec::profile::shipped;
use store::StoreError;
use store::obj::header::text_of;
use store::obj::lock::JobHandle;
use store::obj::{NameContext, ops, types};

fn sysval(n: &str) -> String {
    if n == "QSECURITY" {
        "40".into()
    } else {
        String::new()
    }
}

#[test]
fn checked_created_indexed_cached() {
    let (mut store, qsys) = open_new();
    let g = SecGuard::new(Some(qsys));
    g.set_sysval("QCRTAUT", "*USE");
    store.set_guard(Some(g.clone()));
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    let secret = create(&mut t, app, "SECRET", types::DTAARA, "QSECOFR");
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "BOB")])).unwrap();
    grant::set_public(&mut t, secret, Some(Auth::EXCLUDE)).unwrap();
    grant::set_public(&mut t, app, Some(Auth::CHANGE)).unwrap();
    let bob = Rc::new(RefCell::new(Session::new(
        Principal::load(&mut t, qsys, "BOB", 40).unwrap(),
    )));

    // BOB cannot delete, describe or rename SECRET.
    let e = as_job(&bob, || ops::delete(&mut t, JobHandle(7), secret)).unwrap_err();
    assert!(
        matches!(
            e,
            StoreError::NotAuthorized {
                msgid: "CPF2189",
                ..
            }
        ),
        "{e}"
    );
    assert!(as_job(&bob, || ops::describe(&mut t, secret)).is_err());
    let f = take_failures();
    assert_eq!(f.len(), 2);
    assert_eq!(f[0].user, "BOB");
    assert_eq!(f[0].op, "obj.delete");
    assert_eq!(f[0].name, "SECRET");

    // BOB creates in APPLIB: he owns it, *PUBLIC is QCRTAUT, it is indexed.
    let mine = as_job(&bob, || {
        ops::create(
            &mut t,
            &ops::CreateParams {
                library: app,
                name: Name::parse("MINE").unwrap(),
                type_code: types::DTAARA,
                attribute: String::new(),
                text: String::new(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
    })
    .unwrap();
    let h = grant::header(&mut t, mine).unwrap();
    assert_eq!(text_of(&h.owner), "BOB");
    assert_eq!(Auth::of(h.public_authority), Auth::USE);
    assert!(
        index::list(&mut t, qsys, "BOB", Kind::Owned)
            .unwrap()
            .iter()
            .any(|(i, _)| *i == mine)
    );

    // A library BOB cannot use: CPF9820 when he names it.
    grant::set_public(&mut t, app, Some(Auth::EXCLUDE)).unwrap();
    let names = NameContext::default();
    let e = as_job(&bob, || {
        ops::resolve(
            &mut t,
            qsys,
            &names,
            LibSpec::Name(Name::parse("APPLIB").unwrap()),
            Name::parse("MINE").unwrap(),
            types::DTAARA,
        )
    })
    .unwrap_err();
    assert!(
        matches!(
            e,
            StoreError::NotAuthorized {
                msgid: "CPF9820",
                ..
            }
        ),
        "{e}"
    );
    grant::set_public(&mut t, app, Some(Auth::CHANGE)).unwrap();

    // He owns MINE, so he may delete it; the indexes drop it.
    as_job(&bob, || ops::describe(&mut t, mine)).unwrap();
    as_job(&bob, || ops::describe(&mut t, mine)).unwrap();
    assert!(bob.borrow().stats.hits >= 1);
    as_job(&bob, || ops::delete(&mut t, JobHandle(7), mine)).unwrap();
    assert!(
        !index::list(&mut t, qsys, "BOB", Kind::Owned)
            .unwrap()
            .iter()
            .any(|(i, _)| *i == mine)
    );

    // A private grant is seen at once: the generation changed.
    grant::set_private(&mut t, qsys, secret, "BOB", Some(Auth::ALL)).unwrap();
    as_job(&bob, || ops::describe(&mut t, secret)).unwrap();
    // The system is never checked.
    grant::set_private(&mut t, qsys, secret, "BOB", Some(Auth::EXCLUDE)).unwrap();
    as_job(&bob, || as_system(|| ops::describe(&mut t, secret))).unwrap();

    assert_eq!(index::verify(&mut t, qsys).unwrap(), Vec::<String>::new());
}
