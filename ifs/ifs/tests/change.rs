// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The change counter: every kind of change raises it, reads do not,
//! and a value is never seen twice, across delete and recreate and
//! across a restart.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{Sys, sys, user};
use ifs::auth::data::DataAuth;
use ifs::backend::mem::MemBackend;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use ifs::ops::{Attr, ChgAut, Config, Ctx, Ifs, OpenArgs, RenameMode, Who};
use ifs::vfs::qsys::MemQsys;
use sec::auth::bits::Auth;

fn chg(s: &Sys, cx: &Ctx<'_>, p: &str) -> u64 {
    s.ifs.lstat(cx, p).unwrap().change
}

/// `f` changes `p`: its counter rises.
fn rises(s: &Sys, cx: &Ctx<'_>, p: &str, f: impl FnOnce()) -> u64 {
    let before = chg(s, cx, p);
    f();
    let after = chg(s, cx, p);
    assert!(after > before, "{p}: {before} then {after}");
    after
}

#[test]
fn data_changes_raise_it() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/home/f", b"one");
    assert!(chg(&s, &cx, "/home/f") > 0);
    let fd = s.ifs.open(&cx, "/home/f", OpenArgs::new(O_WRONLY)).unwrap();
    rises(&s, &cx, "/home/f", || {
        s.ifs.write(&cx, fd, b"x").unwrap();
    });
    rises(&s, &cx, "/home/f", || {
        s.ifs.pwrite(&cx, fd, b"y", 5).unwrap();
    });
    rises(&s, &cx, "/home/f", || s.ifs.ftruncate(&cx, fd, 1).unwrap());
    // The descriptor sees what the path sees.
    assert_eq!(s.ifs.fstat(&cx, fd).unwrap().change, chg(&s, &cx, "/home/f"));
    s.ifs.close(&cx, fd).unwrap();
    rises(&s, &cx, "/home/f", || {
        let fd = s
            .ifs
            .open(&cx, "/home/f", OpenArgs::new(O_WRONLY | O_TRUNC))
            .unwrap();
        s.ifs.close(&cx, fd).unwrap();
    });
}

#[test]
fn reads_leave_it() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/home/f", b"data");
    let c = chg(&s, &cx, "/home/f");
    let d = chg(&s, &cx, "/home");
    assert_eq!(s.get(&cx, "/home/f"), b"data");
    let fd = s.ifs.open(&cx, "/home/f", OpenArgs::new(O_RDONLY)).unwrap();
    s.ifs.close(&cx, fd).unwrap();
    s.ifs.stat(&cx, "/home/f").unwrap();
    s.ifs.read_dir(&cx, "/home").unwrap();
    // Changing the owner to the owner is no change.
    s.ifs.chgown(&cx, "/home/f", "QSECOFR", false).unwrap();
    assert_eq!(chg(&s, &cx, "/home/f"), c);
    assert_eq!(chg(&s, &cx, "/home"), d);
}

#[test]
fn attribute_changes_raise_it() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/home/f", b"one");
    let f = "/home/f";
    rises(&s, &cx, f, || s.ifs.chgown(&cx, f, "BOB", false).unwrap());
    rises(&s, &cx, f, || s.ifs.chgpgp(&cx, f, Some("DEV"), false).unwrap());
    rises(&s, &cx, f, || s.ifs.chown(&cx, f, 101, u32::MAX).unwrap());
    rises(&s, &cx, f, || s.ifs.chmod(&cx, f, 0o600).unwrap());
    rises(&s, &cx, f, || {
        s.ifs
            .chgaut(
                &cx,
                f,
                &ChgAut {
                    who: vec![Who::User("ZED".into())],
                    data: DataAuth::parse("*R"),
                    ..ChgAut::default()
                },
            )
            .unwrap();
    });
    rises(&s, &cx, f, || s.ifs.rvkaut(&cx, f, "ZED").unwrap());
    rises(&s, &cx, f, || {
        s.ifs.set_attr(&cx, f, &Attr::Ccsid(37), false).unwrap();
    });
    rises(&s, &cx, f, || {
        s.ifs.set_attr(&cx, f, &Attr::Text("t".into()), false).unwrap();
    });
}

#[test]
fn names_raise_the_object_and_its_directories() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs.mkdir(&cx, "/home/a", 0o755).unwrap();
    s.ifs.mkdir(&cx, "/home/b", 0o755).unwrap();
    rises(&s, &cx, "/home/a", || s.put(&cx, "/home/a/f", b"x"));
    let a = chg(&s, &cx, "/home/a");
    let b = chg(&s, &cx, "/home/b");
    let f = chg(&s, &cx, "/home/a/f");
    s.ifs
        .rename(&cx, "/home/a/f", "/home/b/g", RenameMode::Keep)
        .unwrap();
    assert!(chg(&s, &cx, "/home/a") > a);
    assert!(chg(&s, &cx, "/home/b") > b);
    assert!(chg(&s, &cx, "/home/b/g") > f);
    // A rename in one directory.
    rises(&s, &cx, "/home/b", || {
        s.ifs
            .rename(&cx, "/home/b/g", "/home/b/h", RenameMode::Keep)
            .unwrap();
    });
    rises(&s, &cx, "/home/b", || s.ifs.link(&cx, "/home/b/h", "/home/b/i").unwrap());
    rises(&s, &cx, "/home/b", || s.ifs.unlink(&cx, "/home/b/i").unwrap());
    rises(&s, &cx, "/home/b", || s.ifs.symlink(&cx, "h", "/home/b/l").unwrap());
    rises(&s, &cx, "/home/b", || s.ifs.mkdir(&cx, "/home/b/d", 0o755).unwrap());
    rises(&s, &cx, "/home/b", || s.ifs.rmdir(&cx, "/home/b/d").unwrap());
    rises(&s, &cx, "/home", || s.ifs.mkdir(&cx, "/home/c", 0o755).unwrap());
    // A move moves the counter up too.
    let l = chg(&s, &cx, "/home/b/l");
    s.ifs.mov(&cx, "/home/b/l", "/home/c/l", false).unwrap();
    assert!(chg(&s, &cx, "/home/c/l") > l);
}

#[test]
fn values_are_never_reused() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    let mut seen = Vec::new();
    for _ in 0..3 {
        s.put(&cx, "/home/f", b"same");
        seen.push(chg(&s, &cx, "/home/f"));
        s.ifs.unlink(&cx, "/home/f").unwrap();
    }
    assert!(seen.windows(2).all(|w| w[1] > w[0]), "{seen:?}");
    // A copy is a new object with a value of its own.
    s.put(&cx, "/home/f", b"same");
    s.ifs.copy(&cx, "/home/f", "/home/g", ifs::ops::CopyOpts::default()).unwrap();
    assert!(chg(&s, &cx, "/home/g") > chg(&s, &cx, "/home/f"));
}

#[test]
fn a_restart_continues_above() {
    let b = MemBackend::new();
    let env = common::env();
    let audit = ifs::ops::audit::MemAudit::default();
    let p = user("QSECOFR");
    let cx = |owner| Ctx {
        owner: ifs::ops::Owner(owner),
        principal: &p,
        env: &env,
        audit: &audit,
        job_ccsid: 37,
    };
    let make = || Ifs::new(b.clone(), Box::new(MemQsys::new(Auth::USE)), Config::default());
    let one = make();
    one.ensure_layout(&env).unwrap();
    one.mkdir(&cx(1), "/home/a", 0o755).unwrap();
    let first = one.lstat(&cx(1), "/home/a").unwrap().change;
    drop(one);
    let two = make();
    two.chmod(&cx(1), "/home/a", 0o700).unwrap();
    let second = two.lstat(&cx(1), "/home/a").unwrap().change;
    assert!(second > first + ifs::meta::change::BLOCK - 1, "{first} {second}");
}

#[test]
fn a_record_without_a_counter_reads_zero_and_counts_on() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.put(&cx, "/home/f", b"x");
    // A record written before counters: no tag 21.
    let b = s.ifs.backend();
    let raw = ifs::backend::Backend::xattr_get(b, "root/home/f", ifs::meta::XATTR)
        .unwrap()
        .unwrap();
    let mut m = ifs::meta::codec::decode(&raw).unwrap();
    m.change = 0;
    ifs::backend::Backend::xattr_set(b, "root/home/f", ifs::meta::XATTR, &ifs::meta::codec::encode(&m))
        .unwrap();
    assert_eq!(chg(&s, &cx, "/home/f"), 0);
    rises(&s, &cx, "/home/f", || {
        let fd = s.ifs.open(&cx, "/home/f", OpenArgs::new(O_WRONLY)).unwrap();
        s.ifs.write(&cx, fd, b"y").unwrap();
        s.ifs.close(&cx, fd).unwrap();
    });
}

#[test]
fn qsys_lib_answers_its_change_time() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    let c = s.ifs.lstat(&cx, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE").unwrap();
    assert_eq!(c.change, u64::try_from(c.mtime).unwrap());
    // Created with O_CREAT on a new name: a new file with a value.
    let fd = s
        .ifs
        .open(&cx, "/home/n", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o600))
        .unwrap();
    s.ifs.close(&cx, fd).unwrap();
    assert!(chg(&s, &cx, "/home/n") > 0);
}
