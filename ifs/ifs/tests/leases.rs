// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Leases across owners through the operations: which ones break a
//! lease another owner holds, defer until it reports, and leave it.

#![allow(clippy::many_single_char_names, clippy::items_after_statements)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{env, sys, user};
use ifs::Errno;
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::lease::defer::deferring;
use ifs::lease::state::{H, R, RWH, W};
use ifs::lock::range::RangeKind;
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::flags::{F_SETLK, O_RDONLY, O_RDWR, O_SHARE_NONE, O_SHARE_RDWR, O_TRUNC, SEEK_SET};
use ifs::ops::{Attr, AttrId, Ctx, OpenArgs, Owner, RenameMode};

const KEY: [u8; 32] = [7; 32];

fn file_id(s: &common::Sys, cx: &Ctx<'_>, path: &str) -> FileId {
    s.ifs
        .get_attrs(cx, path, &[AttrId::FileId], true)
        .unwrap()
        .into_iter()
        .find_map(|a| match a {
            Attr::FileId(id) => Some(id),
            _ => None,
        })
        .unwrap()
}

/// Owner 1 holds `state` on `/tmp/f` as tag 5.
fn leased(s: &common::Sys, cx: &Ctx<'_>, state: u8) -> u8 {
    let id = file_id(s, cx, "/tmp/f");
    s.ifs.lease_grant(cx, "/tmp/f", id, 5, KEY, state).unwrap()
}

#[test]
fn every_changing_operation_of_another_owner_breaks_and_defers() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    type Op = fn(&common::Sys, &Ctx<'_>) -> ifs::Result<()>;
    let ops: [(&str, Op, u8); 4] = [
        (
            "read open",
            |s, b| s.ifs.open(b, "/tmp/f", OpenArgs::new(O_RDONLY)).map(drop),
            R | H,
        ),
        (
            "truncating open",
            |s, b| {
                s.ifs
                    .open(b, "/tmp/f", OpenArgs::new(O_RDWR | O_TRUNC))
                    .map(drop)
            },
            0,
        ),
        ("unlink", |s, b| s.ifs.unlink(b, "/tmp/f"), R | W),
        (
            "rename",
            |s, b| s.ifs.rename(b, "/tmp/f", "/tmp/h", RenameMode::Keep),
            R | W,
        ),
    ];
    for (what, op, keep) in ops {
        s.put(&a, "/tmp/f", b"data");
        assert_eq!(leased(&s, &a, RWH), RWH, "{what}");
        let (r, d) = deferring(true, || op(&s, &b));
        assert_eq!(r, Err(Errno::EAGAIN), "{what}");
        let d = d.unwrap_or_else(|| panic!("{what}: no deferral"));
        assert_eq!(
            s.ifs.lease_orders(&a, Duration::ZERO).unwrap(),
            [(5, keep)],
            "{what}"
        );
        // Nothing happened yet.
        assert_eq!(s.get(&a, "/tmp/f"), b"data", "{what}");
        s.ifs.lease_report(&a, 5, &[(KEY, keep)]);
        assert!(s.ifs.leases().ready(&d), "{what}");
        assert_eq!(deferring(true, || op(&s, &b)).1, None, "{what}");
        s.ifs.lease_report(&a, 5, &[]);
        let _ = s.ifs.rename(&a, "/tmp/h", "/tmp/f", RenameMode::Unlink);
        s.ifs.release_owner(Owner(2));
    }
}

/// An open breaks write caching first; writes, truncations and locks
/// through it then break the rest without waiting (nothing is cached for
/// writing any more).
#[test]
fn writes_truncations_and_locks_break_to_none() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    type Op = fn(&common::Sys, &Ctx<'_>, i32) -> ifs::Result<()>;
    let ops: [(&str, Op); 3] = [
        ("write", |s, b, fd| s.ifs.write(b, fd, b"x").map(drop)),
        ("truncate", |s, b, fd| s.ifs.ftruncate(b, fd, 1)),
        ("lock", |s, b, fd| {
            s.ifs
                .lock_range(b, fd, F_SETLK, RangeKind::Write, SEEK_SET, 0, 1)
                .map(drop)
        }),
    ];
    for (what, op) in ops {
        s.put(&a, "/tmp/f", b"data");
        assert_eq!(leased(&s, &a, RWH), RWH, "{what}");
        let (fd, _) = deferring(false, || {
            s.ifs
                .open(&b, "/tmp/f", OpenArgs::new(O_RDWR | O_SHARE_RDWR))
        });
        let fd = fd.unwrap();
        assert_eq!(
            s.ifs.lease_orders(&a, Duration::ZERO).unwrap(),
            [(5, R | H)],
            "{what}"
        );
        s.ifs.lease_report(&a, 5, &[(KEY, R | H)]);
        assert_eq!(deferring(true, || op(&s, &b, fd)), (Ok(()), None), "{what}");
        assert_eq!(
            s.ifs.lease_orders(&a, Duration::ZERO).unwrap(),
            [(5, 0)],
            "{what}"
        );
        s.ifs.lease_report(&a, 5, &[]);
        s.ifs.release_owner(Owner(2));
    }
}

#[test]
fn the_holders_own_calls_and_reads_break_nothing() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/f", b"data");
    leased(&s, &a, RWH);
    let fd = s.ifs.open(&a, "/tmp/f", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(deferring(true, || s.ifs.write(&a, fd, b"x")).1, None);
    assert_eq!(deferring(true, || s.ifs.stat(&b, "/tmp/f")).1, None);
    assert!(s.ifs.lease_orders(&a, Duration::ZERO).unwrap().is_empty());
}

#[test]
fn grants_see_other_owners_opens_and_the_file_they_name() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/f", b"data");
    s.put(&a, "/tmp/other", b"");
    let fd = s.ifs.open(&b, "/tmp/f", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(leased(&s, &a, RWH), R | H);
    s.ifs.close(&b, fd).unwrap();
    // A name that now holds another file gets nothing.
    let id = file_id(&s, &a, "/tmp/other");
    assert_eq!(
        s.ifs
            .lease_grant(&a, "/tmp/f", id, 6, [1; 32], RWH)
            .unwrap(),
        0
    );
}

#[test]
fn a_share_mode_conflict_breaks_handle_caching_first() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/f", b"data");
    leased(&s, &a, R | H);
    let fd = s
        .ifs
        .open(&a, "/tmp/f", OpenArgs::new(O_RDONLY | O_SHARE_NONE))
        .unwrap();
    let (r, d) = deferring(true, || {
        s.ifs
            .open(&b, "/tmp/f", OpenArgs::new(O_RDONLY | O_SHARE_RDWR))
    });
    assert_eq!(r, Err(Errno::EAGAIN));
    assert_eq!(s.ifs.lease_orders(&a, Duration::ZERO).unwrap(), [(5, R)]);
    // The client closes what it cached.
    s.ifs.close(&a, fd).unwrap();
    s.ifs.lease_report(&a, 5, &[(KEY, R)]);
    assert!(s.ifs.leases().ready(&d.unwrap()));
    assert!(
        s.ifs
            .open(&b, "/tmp/f", OpenArgs::new(O_RDONLY | O_SHARE_RDWR))
            .is_ok()
    );
}

#[test]
fn a_copy_breaks_without_waiting() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let b = s.cx(&p, 2);
    s.put(&a, "/tmp/f", b"data");
    s.put(&a, "/tmp/src", b"new");
    leased(&s, &a, RWH);
    let opts = ifs::ops::CopyOpts {
        replace: true,
        ..Default::default()
    };
    assert_eq!(
        deferring(true, || s.ifs.copy(&b, "/tmp/src", "/tmp/f", opts)),
        (Ok(()), None)
    );
    assert!(!s.ifs.lease_orders(&a, Duration::ZERO).unwrap().is_empty());
}

/// Two callers over one IFS on threads: the writer's call waits until the
/// holder, polling for orders on a thread of its own, reports the break.
#[test]
fn a_local_caller_waits_for_the_holders_report() {
    let s = sys();
    let env: Arc<MemEnv> = Arc::new(env());
    let audit = Arc::new(MemAudit::default());
    let ifs = Arc::new(s.ifs);
    let client = move |owner| LocalIfs::<MemBackend> {
        ifs: ifs.clone(),
        env: env.clone(),
        audit: audit.clone(),
        principal: user("QSECOFR"),
        owner: Owner(owner),
        job_ccsid: 37,
    };
    let holder = client(1);
    let fd = holder
        .open(
            "/tmp/f",
            OpenArgs::new(O_RDWR | ifs::ops::flags::O_CREAT).mode(0o644),
        )
        .unwrap();
    let id = holder
        .get_attrs("/tmp/f", &[AttrId::FileId], true)
        .unwrap()
        .into_iter()
        .find_map(|a| match a {
            Attr::FileId(id) => Some(id),
            _ => None,
        })
        .unwrap();
    assert_eq!(holder.lease_grant("/tmp/f", id, 5, KEY, RWH).unwrap(), RWH);
    let polling = client.clone();
    let poller = std::thread::spawn(move || {
        let poll = polling(1);
        let orders = poll.lease_orders(5_000).unwrap();
        assert_eq!(orders, [(5, R | H)]);
        // Write back what the client cached, then give the lease up.
        poll.pwrite(fd, b"cached", 0).unwrap();
        poll.lease_report(5, &[]).unwrap();
    });
    let writer = client(2);
    let wfd = writer.open("/tmp/f", OpenArgs::new(O_RDWR)).unwrap();
    writer.pwrite(wfd, b"!", 6).unwrap();
    poller.join().unwrap();
    assert_eq!(writer.pread(wfd, 100, 0).unwrap(), b"cached!");
}
