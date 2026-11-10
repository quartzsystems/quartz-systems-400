// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::*;
use crate::model::attrs::Dialect;
use crate::model::share::{Kind, Permission, Qualified, SpoolType};

fn host() -> MemHost {
    let mut h = MemHost::new("QS400");
    h.paths.push(("/home/docs".into(), "BOB".into()));
    h.outqs
        .push((Qualified::new("PRT01", "QUSRSYS"), "QSYS".into()));
    h.autls.push("NSAUTL".into());
    h
}

#[test]
fn shares_read_back_as_written() {
    let mut h = host();
    let mut s = Share::file("DOCS", "/home/docs", "Documents");
    if let Kind::File(f) = &mut s.kind {
        f.permission = Permission::ReadOnly;
        f.max_users = 5;
        f.text_conversion = true;
        f.text_ccsid = 819;
        f.extensions = vec!["TXT".into(), "CSV".into()];
    }
    s.encryption_required = true;
    s.autl = "NSAUTL".into();
    add_share(&mut h, &s).expect("adds");
    let mut p = Share::print("LASER", Qualified::new("PRT01", "QUSRSYS"), "Laser");
    if let Kind::Print(ps) = &mut p.kind {
        ps.driver = "Generic PCL".into();
        ps.spool_type = SpoolType::UserAscii;
    }
    add_share(&mut h, &p).expect("adds the printer");
    let all = list_shares(&mut h, 37).expect("lists");
    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["DOCS", "LASER", "QIBM"]);
    assert_eq!(all[0], s);
    assert_eq!(all[1], p);
    assert_eq!(share(&mut h, "laser", 37).expect("one"), Some(p.clone()));
    assert_eq!(share(&mut h, "NONE", 37).expect("none"), None);

    // Changing back to no extensions and no list.
    let mut s2 = s.clone();
    if let Kind::File(f) = &mut s2.kind {
        f.extensions.clear();
    }
    s2.autl.clear();
    change_share(&mut h, &s2).expect("changes");
    assert_eq!(share(&mut h, "DOCS", 37).expect("one"), Some(s2));

    remove_share(&mut h, "LASER").expect("removes");
    let e = remove_share(&mut h, "LASER").expect_err("gone");
    assert_eq!((e.id.as_str(), e.bin4(22)), ("CPFB68A", 2));
}

#[test]
fn a_refusal_is_the_apis() {
    let mut h = host();
    h.caller.iosyscfg = false;
    h.caller.user = "ANN".into();
    let e = add_share(&mut h, &Share::file("DOCS", "/home/docs", "x")).expect_err("not owner");
    assert!(e.not_authorized());
    assert_eq!(e.text_at(0, 10), "QZLSADFS");
}

#[test]
fn attributes_change_only_what_differs() {
    let mut h = host();
    h.profiles.push(("GUEST".into(), Vec::new()));
    let before = read_attributes(&mut h).expect("reads");
    assert_eq!(before.current, before.pending);
    assert_eq!(before.current, Settings::new("QS400"));
    let mut to = before.pending.clone();
    to.domain = "WORKGRP".into();
    to.guest = "GUEST".into();
    to.idle_timeout = 900;
    to.min_dialect = Dialect::Smb30;
    to.allow_netbios = true;
    change_attributes(&mut h, &before.pending, &to).expect("changes");
    let after = read_attributes(&mut h).expect("reads");
    assert_eq!(after.pending, to);
    assert_eq!(after.current, before.current);
    // Nothing to change calls nothing.
    let saves = h.saves;
    change_attributes(&mut h, &to, &to).expect("nothing");
    assert_eq!(h.saves, saves);
    // A value the API refuses comes back as its message.
    let mut bad = to.clone();
    bad.oplock_timeout = 9999;
    let e = change_attributes(&mut h, &to, &bad).expect_err("refused");
    assert_eq!(e.id, "CPFB686");
}

#[test]
fn users_and_the_server() {
    let mut h = host();
    h.profiles.push(("BOB".into(), Vec::new()));
    h.disabled.push("BOB".into());
    assert_eq!(disabled_users(&mut h).expect("lists"), ["BOB"]);
    enable_users(&mut h, &["BOB".into()]).expect("enables");
    assert!(disabled_users(&mut h).expect("lists").is_empty());
    assert_eq!(start(&mut h, true).expect_err("no job").id, "CPFB680");
    assert_eq!(end(&mut h).expect_err("no job").id, "CPFB680");
    assert_eq!(
        end_session(&mut h, 7).expect_err("not active").id,
        "CPFB68E"
    );
}
