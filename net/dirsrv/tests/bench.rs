// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 100 000 adds, resolves and index lookups, timed:
//! `cargo test --release -p dirsrv --test bench -- --ignored --nocapture`.

#![allow(clippy::many_single_char_names, clippy::cast_precision_loss)]

mod common;

use std::time::Instant;

use common::{NORM, dn, ou, rdn, root, system};
use dirsrv::store::{Attr, Entry};

#[test]
#[ignore = "a bench: run by hand"]
fn hundred_thousand_entries() {
    let n = 100_000usize;
    let mut sys = system(131_072);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let s = db
        .add_suffix(&mut t, &NORM, &dn("o=bench"), &root("o=bench"))
        .unwrap();
    let p = db
        .add(&mut t, &NORM, s, &rdn("ou=users"), &ou("users"))
        .unwrap();
    t.commit().unwrap();
    let start = Instant::now();
    for batch in 0..n / 1000 {
        let mut t = sys.store.begin().unwrap();
        for i in batch * 1000..(batch + 1) * 1000 {
            let uid = format!("u{i}");
            let e = Entry::new(
                &format!("uid={uid}"),
                vec![
                    Attr::text("objectClass", &["top", "account"]),
                    Attr::text("uid", &[&uid]),
                ],
            );
            db.add(&mut t, &NORM, p, &rdn(&format!("uid={uid}")), &e)
                .unwrap();
        }
        t.commit().unwrap();
    }
    let add = start.elapsed();
    let start = Instant::now();
    let mut t = sys.store.begin().unwrap();
    for i in (0..n).step_by(7) {
        let d = format!("uid=u{i},ou=users,o=bench");
        assert!(db.resolve(&mut t, &NORM, &dn(&d)).unwrap().entry.is_some());
    }
    let resolve = start.elapsed();
    let start = Instant::now();
    for i in (0..n).step_by(7) {
        let v = format!("u{i}");
        let hit = db
            .equal(&mut t, &NORM, "uid", v.as_bytes(), None, 2)
            .unwrap()
            .unwrap();
        assert_eq!(hit.len(), 1);
    }
    let lookup = start.elapsed();
    t.commit().unwrap();
    let per = |d: std::time::Duration, k: usize| d.as_secs_f64() * 1e6 / k as f64;
    println!(
        "dirdb: {n} adds {:.1} us each, {} resolves {:.1} us, {} uid lookups {:.1} us",
        per(add, n),
        n / 7 + 1,
        per(resolve, n / 7 + 1),
        n / 7 + 1,
        per(lookup, n / 7 + 1)
    );
}
