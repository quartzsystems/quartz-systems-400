// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory watches across owners: which operations a watcher hears,
//! as what, for its directory and (a tree watch) below it.

#![allow(clippy::many_single_char_names)]

mod common;

use std::time::Duration;

use common::{sys, user};
use ifs::Errno;
use ifs::lease::defer::deferring;
use ifs::lease::notify::{ChangeKind, DirEvent, QUEUE};
use ifs::ops::flags::{O_RDWR, O_TRUNC};
use ifs::ops::{Attr, Ctx, OpenArgs, RenameMode};

fn ev(name: &str, kind: ChangeKind, dir: bool) -> DirEvent {
    DirEvent {
        name: name.into(),
        kind,
        dir,
    }
}

fn heard(s: &common::Sys, cx: &Ctx<'_>) -> Vec<DirEvent> {
    let news = s.ifs.lease_orders(cx, Duration::ZERO).unwrap();
    news.changes.into_iter().flat_map(|(_, v)| v).collect()
}

#[test]
fn a_watcher_hears_every_change_to_its_directory() {
    let s = sys();
    let p = user("QSECOFR");
    let (w, o) = (s.cx(&p, 1), s.cx(&p, 2));
    s.ifs.mkdir(&o, "/tmp/w", 0o777).unwrap();
    s.ifs.watch_add(&w, "/tmp/w", false).unwrap();
    s.put(&o, "/tmp/w/a", b"x");
    s.ifs.mkdir(&o, "/tmp/w/d", 0o777).unwrap();
    let fd = s
        .ifs
        .open(&o, "/tmp/w/a", OpenArgs::new(O_RDWR | O_TRUNC))
        .unwrap();
    s.ifs.close(&o, fd).unwrap();
    s.ifs.chmod(&o, "/tmp/w/a", 0o600).unwrap();
    s.ifs
        .set_attr(&o, "/tmp/w/a", &Attr::PcHidden(true), true)
        .unwrap();
    s.ifs.utime(&o, "/tmp/w/a", None).unwrap();
    s.ifs
        .rename(&o, "/tmp/w/a", "/tmp/w/b", RenameMode::Keep)
        .unwrap();
    s.ifs
        .rename(&o, "/tmp/w/b", "/tmp/out", RenameMode::Keep)
        .unwrap();
    s.ifs.rmdir(&o, "/tmp/w/d").unwrap();
    s.ifs.symlink(&o, "x", "/tmp/w/l").unwrap();
    s.ifs.unlink(&o, "/tmp/w/l").unwrap();
    // Outside the directory: not heard.
    s.put(&o, "/tmp/elsewhere", b"y");
    assert_eq!(
        heard(&s, &w),
        [
            ev("a", ChangeKind::Added, false),
            ev("a", ChangeKind::Modified, false),
            ev("d", ChangeKind::Added, true),
            ev("a", ChangeKind::Modified, false),
            ev("a", ChangeKind::Attributes, false),
            ev("a", ChangeKind::Attributes, false),
            ev("a", ChangeKind::Attributes, false),
            ev("b", ChangeKind::Renamed { from: "a".into() }, false),
            ev("b", ChangeKind::Removed, false),
            ev("d", ChangeKind::Removed, true),
            ev("l", ChangeKind::Added, false),
            ev("l", ChangeKind::Removed, false),
        ]
    );
}

#[test]
fn a_tree_watch_hears_below_except_its_own_changes() {
    let s = sys();
    let p = user("QSECOFR");
    let (w, o) = (s.cx(&p, 1), s.cx(&p, 2));
    s.ifs.mkdir(&o, "/tmp/t", 0o777).unwrap();
    s.ifs.mkdir(&o, "/tmp/t/sub", 0o777).unwrap();
    s.ifs.watch_add(&w, "/tmp/t", true).unwrap();
    let flat = s.cx(&p, 3);
    s.ifs.watch_add(&flat, "/tmp/t", false).unwrap();
    s.put(&o, "/tmp/t/sub/f", b"1");
    s.put(&w, "/tmp/t/sub/mine", b"2");
    s.put(&w, "/tmp/t/top", b"3");
    assert_eq!(
        heard(&s, &w),
        [
            ev("sub/f", ChangeKind::Added, false),
            ev("sub/f", ChangeKind::Modified, false),
            ev("top", ChangeKind::Added, false),
            ev("top", ChangeKind::Modified, false),
        ]
    );
    assert_eq!(
        heard(&s, &flat),
        [
            ev("top", ChangeKind::Added, false),
            ev("top", ChangeKind::Modified, false)
        ]
    );
}

#[test]
fn a_watched_directory_moved_or_removed_tells_its_watcher() {
    let s = sys();
    let p = user("QSECOFR");
    let (w, o) = (s.cx(&p, 1), s.cx(&p, 2));
    s.ifs.mkdir(&o, "/tmp/m", 0o777).unwrap();
    s.ifs.watch_add(&w, "/tmp/m", false).unwrap();
    s.ifs
        .rename(&o, "/tmp/m", "/tmp/n", RenameMode::Keep)
        .unwrap();
    // The watch follows the directory.
    s.put(&o, "/tmp/n/x", b"");
    s.ifs.unlink(&o, "/tmp/n/x").unwrap();
    s.ifs.rmdir(&o, "/tmp/n").unwrap();
    s.ifs.mkdir(&o, "/tmp/n", 0o777).unwrap();
    s.put(&o, "/tmp/n/y", b"");
    let mut got = heard(&s, &w);
    got.retain(|e| e.kind != ChangeKind::Modified);
    assert_eq!(
        got,
        [
            ev("", ChangeKind::Moved, true),
            ev("x", ChangeKind::Added, false),
            ev("x", ChangeKind::Removed, false),
            ev("", ChangeKind::Deleted, true),
        ]
    );
}

#[test]
fn an_overfull_queue_is_one_lost_and_the_owners_end_takes_its_watches() {
    let s = sys();
    let p = user("QSECOFR");
    let (w, o) = (s.cx(&p, 1), s.cx(&p, 2));
    s.ifs.mkdir(&o, "/tmp/q", 0o777).unwrap();
    s.ifs.watch_add(&w, "/tmp/q", false).unwrap();
    s.put(&o, "/tmp/q/f", b"");
    for i in 0..QUEUE {
        s.ifs
            .chmod(&o, "/tmp/q/f", if i % 2 == 0 { 0o600 } else { 0o644 })
            .unwrap();
    }
    assert_eq!(heard(&s, &w), [ev("", ChangeKind::Lost, true)]);
    s.ifs.chmod(&o, "/tmp/q/f", 0o640).unwrap();
    assert_eq!(heard(&s, &w), [ev("f", ChangeKind::Attributes, false)]);
    s.ifs.release_owner(ifs::ops::Owner(1));
    assert!(!s.ifs.leases().watching());
}

#[test]
fn a_poll_defers_until_a_change_comes_and_watching_needs_authority() {
    let s = sys();
    let p = user("QSECOFR");
    let (w, o) = (s.cx(&p, 1), s.cx(&p, 2));
    s.ifs.mkdir(&o, "/tmp/p", 0o700).unwrap();
    let (r, d) = deferring(true, || s.ifs.lease_orders(&w, Duration::from_secs(5)));
    assert_eq!(r, Err(Errno::EAGAIN));
    let id = s.ifs.watch_add(&w, "/tmp/p", false).unwrap();
    let d = d.unwrap();
    assert!(!s.ifs.leases().ready(&d));
    s.put(&o, "/tmp/p/z", b"");
    assert!(s.ifs.leases().ready(&d));
    s.ifs.watch_end(&w, id);
    assert!(!s.ifs.leases().watching());
    let bob = user("BOB");
    let b = s.cx(&bob, 3);
    assert_eq!(s.ifs.watch_add(&b, "/tmp/p", false), Err(Errno::EACCES));
    assert_eq!(s.ifs.watch_add(&w, "/tmp/p/z", false), Err(Errno::ENOTDIR));
}
