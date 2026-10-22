// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profile commands against a store with the shipped profiles.

mod common;

use common::{NOW, open_new};
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::password::policy::Policy;
use sec::profile::{LmtCpb, SpecialAuth, UserClass, groups, objects, shipped};
use store::Store;
use store::obj::ObjectId;

fn sysval(n: &str) -> String {
    match n {
        "QSECURITY" => "40",
        "QPWDLVL" => "2",
        "QPWDMINLEN" => "6",
        "QPWDMAXLEN" => "128",
        "QPWDRQDDGT" | "QPWDRQDDIF" => "1",
        "QPWDEXPITV" => "*NOMAX",
        "QPWDRULES" => "*PWDSYSVAL",
        _ => "",
    }
    .into()
}

fn setup() -> (Store, ObjectId, Principal) {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, 40, 2, NOW).unwrap();
    let sec = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    t.commit().unwrap();
    (store, qsys, sec)
}

#[test]
fn create_change_and_passwords() {
    let (mut store, qsys, secofr) = setup();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    let mut t = store.begin().unwrap();
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "devs"), ("TEXT", "'Developers'")]),
    )
    .unwrap();
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[
            ("USRPRF", "BOB"),
            ("PASSWORD", "Start123"),
            ("USRCLS", "*PGMR"),
            ("GRPPRF", "DEVS"),
            ("PWDEXP", "*YES"),
            ("LMTCPB", "*PARTIAL"),
            ("MAXSTG", "5000"),
            ("USREXPDATE", "2027-01-31"),
        ]),
    )
    .unwrap();
    let e = usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "BOB")])).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2214"));
    let e = usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "X"), ("GRPPRF", "NOPE")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2204"));
    let e = usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "Y"), ("SUPGRPPRF", "DEVS")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF22A1"));

    let (_, bob) = objects::read(&mut t, qsys, "BOB").unwrap();
    assert_eq!(bob.usrcls, UserClass::Pgmr);
    assert_eq!(bob.spcaut, SpecialAuth::NONE);
    assert_eq!(bob.lmtcpb, LmtCpb::Partial);
    assert!(bob.pwdexp && bob.uid >= groups::FIRST_ID);
    assert!(sec::password::matches(&bob, "Start123", 2));
    assert_eq!(groups::members(&mut t, qsys, "DEVS").unwrap(), vec!["BOB"]);
    assert!(objects::read(&mut t, qsys, "DEVS").unwrap().1.gid >= groups::FIRST_ID);
    let pol = Policy::from_sysvals(&sysval);
    let shown = usrprf::display_basic(&bob, &pol, NOW);
    assert!(
        shown.contains("*PGMR") && shown.contains("2027-01-31"),
        "{shown}"
    );

    // A *SECADM-only user cannot hand out *ALLOBJ.
    usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BOB"), ("SPCAUT", "*SECADM")]),
    )
    .unwrap();
    let bob_p = Principal::load(&mut t, qsys, "BOB", 40).unwrap();
    let bob_env = Env {
        qsys,
        actor: &bob_p,
        now: NOW,
        sysval: &sysval,
    };
    let e = usrprf::crtusrprf(
        &mut t,
        &bob_env,
        &Keywords::new(&[("USRPRF", "EVE"), ("SPCAUT", "*ALLOBJ")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF2222"));
    usrprf::crtusrprf(&mut t, &bob_env, &Keywords::new(&[("USRPRF", "EVE")])).unwrap();

    // CHGPWD and CHKPWD as BOB.
    let mut ok = |_: &sec::profile::UsrPrf, _: &str, _: Option<&str>| Ok(());
    let e = usrprf::chgpwd(&mut t, &bob_env, "Start123", "short", &mut ok).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF4AA7"));
    usrprf::chgpwd(&mut t, &bob_env, "Start123", "Better456", &mut ok).unwrap();
    assert!(!objects::read(&mut t, qsys, "BOB").unwrap().1.pwdexp);
    usrprf::chkpwd(&mut t, &bob_env, "Better456").unwrap();
    assert!(usrprf::chkpwd(&mut t, &bob_env, "Start123").is_err());
    assert_eq!(
        objects::read(&mut t, qsys, "BOB").unwrap().1.signon_invalid,
        1
    );

    let users = usrprf::dspautusr(&mut t, &env, true).unwrap();
    assert!(users.contains("BOB") && users.contains("QSECOFR"));
    let m = usrprf::display_members(&mut t, &env, "DEVS").unwrap();
    assert!(m.contains("BOB"));
}
