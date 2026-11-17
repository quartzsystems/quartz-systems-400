// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operations through the engine, each its own transaction as the
//! service runs them, timed with the store's I/O per operation:
//! `cargo test --release -p dirsrv --test op_bench -- --ignored --nocapture`.

#![allow(clippy::cast_precision_loss)]

mod harness;

use std::time::Instant;

use harness::{T, text};
use ldap::msg::Scope;

fn person(i: usize, n: usize) -> (String, Vec<ldap::msg::Attribute>) {
    let cn = format!("user{i:07}");
    let sn = format!("s{:07}", (i * 7919) % n.max(1));
    (
        format!("cn={cn},o=Acme"),
        vec![
            text(
                "objectClass",
                &["top", "person", "organizationalPerson", "inetOrgPerson"],
            ),
            text("cn", &[&cn]),
            text("sn", &[&sn]),
        ],
    )
}

#[test]
#[ignore = "a bench: run by hand"]
fn adds() {
    let n = 20_000usize;
    let mut t = T::with_blocks(harness::settings(), 262_144);
    let a = t.admin.clone();
    let s0 = t.sys.store.stats();
    let d0 = t.sys.dev.stats();
    let start = Instant::now();
    for i in 0..n {
        let (dn, attrs) = person(i, n);
        t.add_ok(&a, &dn, &attrs);
    }
    let took = start.elapsed();
    let s1 = t.sys.store.stats();
    let d1 = t.sys.dev.stats();
    let per = |x: u64, y: u64| (x - y) as f64 / n as f64;
    println!(
        "adds: {n}, {:.1} us each; per add: {:.2} commits, {:.1} WAL pages, {:.1} images, \
         {:.1} patches, {:.1} device writes, {:.2} flushes; {} checkpoints",
        took.as_secs_f64() * 1e6 / n as f64,
        per(s1.commits, s0.commits),
        per(s1.wal_pages, s0.wal_pages),
        per(s1.images, s0.images),
        per(s1.patches, s0.patches),
        per(d1.writes, d0.writes),
        per(d1.flushes, d0.flushes),
        s1.checkpoints - s0.checkpoints,
    );
    for f in [
        "(cn=user0012345)",
        "(sn=s00001*)",
        "(cn=*12345*)",
        "(cn=*9*)",
        "(&(objectClass=person)(sn=*0123))",
    ] {
        let start = Instant::now();
        let r = t.search(&a, "o=Acme", Scope::Sub, f, &["cn"]);
        println!(
            "search {f}: {} replies, {:.1} ms",
            r.len(),
            start.elapsed().as_secs_f64() * 1e3
        );
    }
}
