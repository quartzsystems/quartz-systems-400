// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign-on: every refusal, the attempt limit, the console's security
//! officer, an expired password changed, the information shown.

#![allow(clippy::many_single_char_names, clippy::match_same_arms)]

mod common;

use common::{NOW, open_new};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::profile::{objects, shipped};
use sec::signon::{self, Attempt, Next, Typed, Values};
use store::txn::Txn;

fn sysval(n: &str) -> String {
    match n {
        "QSECURITY" => "40",
        "QPWDLVL" => "3",
        "QPWDMINLEN" => "6",
        "QPWDMAXLEN" => "128",
        "QPWDRULES" => "*PWDSYSVAL",
        "QPWDEXPITV" => "*NOMAX",
        "QMAXSIGN" => "3",
        "QMAXSGNACN" => "3",
        "QLMTSECOFR" | "QDSPSGNINF" => "1",
        "QLMTDEVSSN" => "0",
        _ => "",
    }
    .into()
}

fn attempt(user: &str, password: &str) -> Attempt {
    Attempt {
        user: user.into(),
        password: password.into(),
        device: "QPADEV0001".into(),
        console: false,
        sessions: 0,
        now: NOW,
        proof: None,
    }
}

fn none(_: &mut Txn<'_>, _: &str) -> Option<Auth> {
    None
}

fn msgid(t: &mut Txn<'_>, qsys: store::obj::ObjectId, a: &Attempt) -> String {
    let v = Values::from_sysvals(&sysval);
    match signon::sign_on(t, qsys, a, &v, &none).unwrap() {
        Ok(_) => "OK".into(),
        Err(r) => r.msgid,
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_path() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
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
        &Keywords::new(&[("USRPRF", "BOB"), ("PASSWORD", "Right123")]),
    )
    .unwrap();
    let v = Values::from_sysvals(&sysval);

    // Unknown user and wrong password read the same.
    assert_eq!(msgid(&mut t, qsys, &attempt("NOBODY", "x")), "CPF1107");
    assert_eq!(msgid(&mut t, qsys, &attempt("BOB", "wrong")), "CPF1107");
    assert_eq!(msgid(&mut t, qsys, &attempt("BOB", "wrong")), "CPF1392");
    let a = attempt("BOB", "wrong");
    let r = signon::sign_on(&mut t, qsys, &a, &v, &none)
        .unwrap()
        .unwrap_err();
    assert_eq!(r.msgid, "CPF1393");
    assert!(r.vary_off);
    assert!(objects::read(&mut t, qsys, "BOB").unwrap().1.disabled);
    assert_eq!(msgid(&mut t, qsys, &attempt("BOB", "Right123")), "CPF1394");
    let events = sec::audit::take();
    assert!(
        events
            .iter()
            .any(|e| e.kind == sec::audit::Kind::Pw && e.get("ETYP") == "U")
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == sec::audit::Kind::Pw && e.get("ETYP") == "P")
    );

    // Enabled again: a good sign-on clears the count and shows it once.
    usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("STATUS", "*ENABLED")]),
    )
    .unwrap();
    let _ = msgid(&mut t, qsys, &attempt("BOB", "wrong"));
    let ok = signon::sign_on(&mut t, qsys, &attempt("BOB", "Right123"), &v, &none)
        .unwrap()
        .unwrap();
    assert_eq!(ok.next, Next::Start);
    assert!(ok.show_info);
    assert_eq!(ok.invalid, 1);
    assert_eq!(ok.inlmnu, "MAIN");
    assert_eq!(
        objects::read(&mut t, qsys, "BOB").unwrap().1.signon_invalid,
        0
    );

    // One session at a time when limited.
    usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("LMTDEVSSN", "*YES")]),
    )
    .unwrap();
    let mut two = attempt("BOB", "Right123");
    two.sessions = 1;
    assert_eq!(msgid(&mut t, qsys, &two), "CPF1220");

    // *NONE cannot sign on.
    assert_eq!(msgid(&mut t, qsys, &attempt("QSYSOPR", "x")), "CPF1118");

    // QSECOFR: expired password, and QLMTSECOFR away from the console.
    assert_eq!(
        msgid(&mut t, qsys, &attempt("QSECOFR", "QSECOFR")),
        "CPF1110"
    );
    let mut at_console = attempt("QSECOFR", "QSECOFR");
    at_console.console = true;
    let ok = signon::sign_on(&mut t, qsys, &at_console, &v, &none)
        .unwrap()
        .unwrap();
    assert_eq!(ok.next, Next::ChangePassword);
    let mut ok_validate = |_: &sec::profile::UsrPrf, _: &str, _: Option<&str>| Ok(());
    let e = signon::change_expired(
        &mut t,
        qsys,
        "QSECOFR",
        Typed {
            current: "QSECOFR",
            new: "Secure99",
            confirm: "Secure98",
        },
        &v,
        NOW,
        &mut ok_validate,
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2361"));
    let done = signon::change_expired(
        &mut t,
        qsys,
        "QSECOFR",
        Typed {
            current: "QSECOFR",
            new: "Secure99",
            confirm: "Secure99",
        },
        &v,
        NOW,
        &mut ok_validate,
    )
    .unwrap();
    assert_eq!(done.next, Next::Start);
    // With *CHANGE to the device, a security officer signs on anywhere.
    let device = |_: &mut Txn<'_>, u: &str| (u == "QSECOFR").then_some(Auth::CHANGE);
    let elsewhere = attempt("QSECOFR", "Secure99");
    assert!(
        signon::sign_on(&mut t, qsys, &elsewhere, &v, &device)
            .unwrap()
            .is_ok()
    );
    // The console never disables QSECOFR.
    for _ in 0..5 {
        let mut bad = attempt("QSECOFR", "nope");
        bad.console = true;
        let _ = signon::sign_on(&mut t, qsys, &bad, &v, &none).unwrap();
    }
    assert!(
        signon::sign_on(&mut t, qsys, &at_console_with("Secure99"), &v, &none)
            .unwrap()
            .is_ok()
    );
}

fn at_console_with(password: &str) -> Attempt {
    let mut a = attempt("QSECOFR", password);
    a.console = true;
    a
}
