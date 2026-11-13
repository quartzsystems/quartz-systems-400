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
    assert_eq!(e.msgid(), Some("CPF2291"));
    usrprf::crtusrprf(&mut t, &bob_env, &Keywords::new(&[("USRPRF", "EVE")])).unwrap();

    // CHGPWD and CHKPWD as BOB.
    let mut ok = |_: &sec::profile::UsrPrf, _: &str, _: Option<&str>| Ok(());
    let e = usrprf::chgpwd(&mut t, &bob_env, "Start123", "short", &mut ok).unwrap_err();
    assert_eq!(e.msgid(), Some("CPF22C2"));
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

#[test]
fn shipped_uids_are_ibms_and_numbers_are_unique() {
    let (mut store, qsys, secofr) = setup();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    let mut t = store.begin().unwrap();
    let uid = |t: &mut store::txn::Txn<'_>, n: &str| objects::read(t, qsys, n).unwrap().1.uid;
    assert_eq!(uid(&mut t, "QSECOFR"), 0);
    assert_eq!(uid(&mut t, "QSYS"), 101);
    assert_eq!(uid(&mut t, "QNFSANON"), 4_294_770_710);
    assert_eq!(uid(&mut t, "QUSER"), 4_294_770_692);
    // Generated numbers stay below the shipped ones and skip theirs.
    usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "ANN")])).unwrap();
    let ann = uid(&mut t, "ANN");
    assert!((groups::FIRST_ID..shipped::RESERVED_UIDS).contains(&ann));
    assert!(![101, 102, 103].contains(&ann), "{ann}");
    // A UID or GID another profile has is refused.
    let e = usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "BEN"), ("UID", &ann.to_string())]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF22CE"));
    let e = usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "ANN"), ("UID", "101")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF22CE"));
    // Its own number again is no conflict.
    usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "ANN"), ("UID", &ann.to_string())]),
    )
    .unwrap();
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "GRP1"), ("GID", "700")]),
    )
    .unwrap();
    let e = usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "GRP2"), ("GID", "700")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF22CE"));
    // QSECOFR keeps 0: changing it is refused, other changes keep it.
    let e = usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "QSECOFR"), ("UID", "500")]),
    )
    .unwrap_err();
    assert_eq!(e.msgid(), Some("CPF224C"));
    usrprf::chgusrprf(
        &mut t,
        &env,
        &Keywords::new(&[("USRPRF", "QSECOFR"), ("TEXT", "'Officer'")]),
    )
    .unwrap();
    assert_eq!(uid(&mut t, "QSECOFR"), 0);
}

#[test]
fn an_older_system_gets_the_shipped_uids_where_free() {
    let (mut store, qsys, _) = setup();
    let mut t = store.begin().unwrap();
    // As before UIDs were fixed: shipped profiles at 0, a user at 101.
    for n in ["QSYS", "QUSER", "QNFSANON"] {
        let (id, mut p) = objects::read(&mut t, qsys, n).unwrap();
        p.uid = 0;
        objects::write(&mut t, id, &p).unwrap();
    }
    let (_, mut bob) = objects::read(&mut t, qsys, "QPGMR").unwrap();
    bob.name = "BOB".into();
    bob.uid = 101;
    objects::create(
        &mut t,
        qsys,
        &bob,
        "QSECOFR",
        store::obj::header::Authority::EXCLUDE,
        NOW,
    )
    .unwrap();
    let (id, mut q) = objects::read(&mut t, qsys, "QSRV").unwrap();
    q.uid = 777;
    objects::write(&mut t, id, &q).unwrap();
    let d = shipped::install(&mut t, qsys, 40, 2, NOW).unwrap();
    assert_eq!(d.numbered, 2);
    let uid = |t: &mut store::txn::Txn<'_>, n: &str| objects::read(t, qsys, n).unwrap().1.uid;
    assert_eq!(uid(&mut t, "QUSER"), 4_294_770_692);
    assert_eq!(uid(&mut t, "QNFSANON"), 4_294_770_710);
    assert_eq!(uid(&mut t, "QSYS"), 0, "BOB holds 101");
    assert_eq!(uid(&mut t, "QSRV"), 777, "a UID it has stays");
}
