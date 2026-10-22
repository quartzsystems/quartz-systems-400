// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Changing authority keeps the objects and the profile indexes in step.

mod common;

use common::{NOW, create, lib, open_new};
use sec::auth::bits::Auth;
use sec::auth::{autl, grant};
use sec::cmd::{Env, Keywords, usrprf};
use sec::auth::principal::Principal;
use sec::profile::index::{self, Kind};
use sec::profile::{objects, shipped};
use store::obj::header::{Authority, text_of};
use store::obj::{auth, types};

fn sysval(n: &str) -> String {
    if n == "QSECURITY" { "40".into() } else { String::new() }
}

#[test]
fn private_owner_group_and_lists() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    let obj = create(&mut t, app, "DATA", types::DTAARA, "QSECOFR");
    let other = create(&mut t, app, "OTHER", types::DTAARA, "QSECOFR");
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    for i in 0..20 {
        usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", &format!("U{i:02}"))])).unwrap();
    }

    // Twenty private authorities: sixteen inline, four in the tree.
    for i in 0..20 {
        let bits = if i % 2 == 0 { Auth::USE } else { Auth::CHANGE };
        grant::set_private(&mut t, qsys, obj, &format!("U{i:02}"), Some(bits)).unwrap();
    }
    let h = grant::header(&mut t, obj).unwrap();
    assert_eq!(h.private_auths.len(), 16);
    assert!(h.private_overflow.is_some());
    assert_eq!(auth::private_list(&mut t, &h).unwrap().len(), 20);
    assert_eq!(grant::private(&mut t, &h, "U19").unwrap(), Some(Auth::CHANGE));
    let gen_before = h.auth_gen;
    // Revoking an inline one pulls one up from the tree.
    grant::set_private(&mut t, qsys, obj, "U00", None).unwrap();
    let h = grant::header(&mut t, obj).unwrap();
    assert_eq!(h.private_auths.len(), 16);
    assert_eq!(auth::private_list(&mut t, &h).unwrap().len(), 19);
    assert!(h.auth_gen > gen_before);
    assert_eq!(grant::private(&mut t, &h, "U00").unwrap(), None);
    assert!(grant::set_private(&mut t, qsys, obj, "NOBODY", Some(Auth::USE)).is_err());
    let idx = index::list(&mut t, qsys, "U03", Kind::Private).unwrap();
    assert_eq!(idx, vec![(obj, u64::from(Auth::CHANGE.0))]);

    // Owner: the new owner's private authority goes, the old owner keeps *ALL.
    grant::set_owner(&mut t, qsys, obj, "U03", false).unwrap();
    let h = grant::header(&mut t, obj).unwrap();
    assert_eq!(text_of(&h.owner), "U03");
    assert_eq!(grant::private(&mut t, &h, "U03").unwrap(), None);
    assert_eq!(grant::private(&mut t, &h, "QSECOFR").unwrap(), Some(Auth::ALL));
    assert!(index::list(&mut t, qsys, "U03", Kind::Owned).unwrap().iter().any(|(i, _)| *i == obj));
    assert!(!index::list(&mut t, qsys, "QSECOFR", Kind::Owned).unwrap().iter().any(|(i, _)| *i == obj));

    // Primary group: a private authority becomes the group's.
    grant::set_group(&mut t, qsys, obj, Some("U05"), true).unwrap();
    let h = grant::header(&mut t, obj).unwrap();
    assert_eq!(text_of(&h.primary_group), "U05");
    assert_eq!(Auth::of(h.group_authority), Auth::CHANGE);
    assert_eq!(grant::private(&mut t, &h, "U05").unwrap(), None);
    assert!(grant::set_group(&mut t, qsys, obj, Some("U03"), true).is_err());
    grant::set_group(&mut t, qsys, obj, None, false).unwrap();
    let h = grant::header(&mut t, obj).unwrap();
    assert_eq!(grant::private(&mut t, &h, "U05").unwrap(), Some(Auth::CHANGE));

    // An authorization list secures two objects.
    autl::create(&mut t, qsys, "PAYROLL", "Payroll", "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
    assert!(autl::create(&mut t, qsys, "PAYROLL", "", "QSECOFR", Authority::EXCLUDE, NOW).is_err());
    let list = autl::find(&mut t, qsys, "PAYROLL").unwrap();
    grant::set_private(&mut t, qsys, list, "U07", Some(Auth::USE)).unwrap();
    grant::set_autl(&mut t, qsys, obj, Some("PAYROLL")).unwrap();
    grant::set_autl(&mut t, qsys, other, Some("PAYROLL")).unwrap();
    grant::set_public(&mut t, other, None).unwrap();
    let mut s = autl::secured(&mut t, qsys, "PAYROLL").unwrap();
    s.sort();
    let mut want = vec![obj, other];
    want.sort();
    assert_eq!(s, want);
    assert!(grant::header(&mut t, other).unwrap().public_authority.0 & Authority::FROM_AUTL != 0);
    grant::set_autl(&mut t, qsys, other, None).unwrap();
    assert_eq!(autl::secured(&mut t, qsys, "PAYROLL").unwrap(), vec![obj]);
    assert_eq!(grant::header(&mut t, other).unwrap().public_authority.0 & Authority::FROM_AUTL, 0);
    assert!(grant::set_autl(&mut t, qsys, obj, Some("NOLIST")).is_err());

    // Objects made after the install are indexed by the guard (next).
    index::rebuild(&mut t, qsys).unwrap();
    assert_eq!(index::verify(&mut t, qsys).unwrap(), Vec::<String>::new());
    assert!(objects::exists(&mut t, qsys, "U19"));
}
