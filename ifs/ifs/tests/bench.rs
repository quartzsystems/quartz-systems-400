// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IFS operations per second through `Ifs` (resolution, authority,
//! the metadata record): create and write 4 KiB, stat, unlink, as a
//! user in a directory the user owns. Ignored by default; run with
//! `cargo test --release -p ifs --test bench -- --ignored --nocapture`
//! and `QS400_IFS_BENCH_FILES=N` (default 20,000). `ifs_ops_linux`
//! runs the same over `LinuxBackend` in `QS400_IFS_TEST_DIR` (root, a
//! file system with `trusted.` attributes, as `tests/linux-xfs.sh`).

#![allow(clippy::cast_precision_loss)]

mod common;

use std::time::Instant;

use ifs::backend::Backend;
use ifs::ops::audit::NoAudit;
use ifs::ops::{Config, Ctx, Ifs, Owner};

fn files() -> usize {
    std::env::var("QS400_IFS_BENCH_FILES").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000)
}

fn run<B: Backend>(what: &str, b: B) {
    let n = files();
    let env = common::env();
    let ifs = Ifs::new(
        b,
        Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::EXCLUDE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    let audit = NoAudit;
    let bob = common::user("BOB");
    let sec = common::user("QSECOFR");
    let cx = |p| Ctx {
        owner: Owner(1),
        principal: p,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    ifs.mkdir(&cx(&sec), "/home/bob", 0o755).unwrap();
    ifs.chgown(&cx(&sec), "/home/bob", "BOB", true).unwrap();
    ifs.mkdir(&cx(&bob), "/home/bob/bench", 0o755).unwrap();
    let data = vec![0x5Au8; 4096];
    let c = cx(&bob);
    let names: Vec<String> = (0..n).map(|i| format!("/home/bob/bench/f{i:06}.dat")).collect();

    let t = Instant::now();
    for p in &names {
        let fd = ifs.creat(&c, p, 0o644).unwrap();
        assert_eq!(ifs.write(&c, fd, &data).unwrap(), data.len());
        ifs.close(&c, fd).unwrap();
    }
    let create = t.elapsed().as_secs_f64();

    let t = Instant::now();
    for p in &names {
        assert_eq!(ifs.stat(&c, p).unwrap().size, 4096);
    }
    let stat = t.elapsed().as_secs_f64();

    let t = Instant::now();
    for p in &names {
        ifs.unlink(&c, p).unwrap();
    }
    let unlink = t.elapsed().as_secs_f64();
    ifs.remove_tree(&cx(&sec), "/home/bob").unwrap();

    let rate = |s: f64| n as f64 / s;
    println!("ifs over {what}: {n} files of 4 KiB in one directory, as a user");
    println!("  create+write+close {create:.3} s  {:.0} files/s", rate(create));
    println!("  stat               {stat:.3} s  {:.0} ops/s", rate(stat));
    println!("  unlink             {unlink:.3} s  {:.0} ops/s", rate(unlink));
}

#[test]
#[ignore = "bench: run in release with --ignored --nocapture"]
fn ifs_ops_mem() {
    run("MemBackend", ifs::backend::mem::MemBackend::new());
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "bench: needs root and a file system with trusted xattrs"]
fn ifs_ops_linux() {
    let dir = std::env::var("QS400_IFS_TEST_DIR").expect("QS400_IFS_TEST_DIR names an empty directory");
    run(&format!("LinuxBackend ({dir})"), ifs::backend::linux::LinuxBackend::open(&dir).unwrap());
}
