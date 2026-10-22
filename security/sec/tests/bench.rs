// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority check's latency in memory, cold and cached:
//! `cargo test --release -p sec --test bench -- --ignored --nocapture`.

mod common;

use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;
use std::time::Instant;

use common::{NOW, create, lib, open_new};
use sec::auth::bits::Auth;
use sec::auth::cache::Session;
use sec::auth::principal::Principal;
use sec::auth::required::Need;
use sec::auth::{grant, search};
use sec::cmd::{Env, Keywords, usrprf};
use sec::profile::shipped;
use store::obj::types;

const USERS: usize = 50;
const OBJECTS: usize = 1000;
/// Private authorities per object: past the sixteen inline ones.
const PRIVATE: usize = 20;

fn sysval(n: &str) -> String {
    if n == "QSECURITY" {
        "40".into()
    } else {
        String::new()
    }
}

#[test]
#[ignore = "a benchmark"]
fn auth_check_latency() {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    shipped::install(&mut t, qsys, 40, 0, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    for i in 0..USERS {
        usrprf::crtusrprf(
            &mut t,
            &env,
            &Keywords::new(&[("USRPRF", &format!("U{i:02}"))]),
        )
        .unwrap();
    }
    let objs: Vec<_> = (0..OBJECTS)
        .map(|i| create(&mut t, app, &format!("D{i:04}"), types::DTAARA, "QSECOFR"))
        .collect();
    for (n, id) in objs.iter().enumerate() {
        grant::set_public(&mut t, *id, Some(Auth::EXCLUDE)).unwrap();
        for u in 0..PRIVATE {
            let user = format!("U{:02}", (n + u) % USERS);
            grant::set_private(&mut t, qsys, *id, &user, Some(Auth::USE)).unwrap();
        }
    }
    t.commit().unwrap();

    let p = Principal::load(&mut store.begin().unwrap(), qsys, "U07", 40).unwrap();
    let mut t = store.begin().unwrap();
    let headers: Vec<_> = objs
        .iter()
        .map(|id| grant::header(&mut t, *id).unwrap())
        .collect();
    let t0 = Instant::now();
    let mut allowed = 0;
    for h in &headers {
        let d = search::check(&mut t, qsys, &p, h, Need::Bits(Auth::USE)).unwrap();
        allowed += usize::from(black_box(d.allowed));
    }
    let cold = t0.elapsed() / u32::try_from(headers.len()).unwrap();

    let s = Rc::new(RefCell::new(Session::new(p)));
    for h in &headers {
        sec::guard::decide(&mut t, qsys, &s, "dtaara.read", h).unwrap();
    }
    let t1 = Instant::now();
    for h in &headers {
        let d = sec::guard::decide(&mut t, qsys, &s, "dtaara.read", h).unwrap();
        black_box(d.allowed);
    }
    let cached = t1.elapsed() / u32::try_from(headers.len()).unwrap();
    let t2 = Instant::now();
    for id in &objs {
        black_box(grant::header(&mut t, *id).unwrap());
    }
    let header = t2.elapsed() / u32::try_from(objs.len()).unwrap();
    println!(
        "auth check: {OBJECTS} objects, {PRIVATE} private authorities each, {USERS} users; U07 allowed on {allowed}"
    );
    println!("  header read: {:.1} us", header.as_secs_f64() * 1e6);
    println!("  cold (the search): {:.1} us", cold.as_secs_f64() * 1e6);
    println!(
        "  cached (the session): {:.1} us",
        cached.as_secs_f64() * 1e6
    );
    assert_eq!(
        allowed,
        OBJECTS * PRIVATE / USERS,
        "U07's share of the private authorities"
    );
}
