// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a file server job asks of the IFS: calls as another user, file
//! IDs that outlive the cache and a restart, locks of sub-owners that
//! wait in the table, share reservations between jobs.

#![allow(clippy::too_many_lines)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{env, sys, user};
use ifs::Errno;
use ifs::backend::mem::MemBackend;
use ifs::client::{Caller, IfsClient, LocalIfs};
use ifs::lease::defer::deferring;
use ifs::lock::Held;
use ifs::lock::range::RangeKind;
use ifs::ops::audit::{AuditKind, FileId, MemAudit};
use ifs::ops::flags::{
    F_GETLK, F_SETLK, F_SETLKW, O_CREAT, O_RDONLY, O_RDWR, O_SHARE_NONE, O_SHARE_RDONLY,
    O_SHARE_RDWR, O_SHARE_WRONLY, O_WRONLY, R_OK, SEEK_SET, W_OK,
};
use ifs::ops::{Config, Ifs, OpenArgs, Owner, RenameMode};
use ifs::vfs::ids;
use ifs::vfs::qsys::MemQsys;
use sec::auth::bits::Auth;

/// A job's client over `ifs`: `QSECOFR`, owner `owner`.
fn job(ifs: &Arc<Ifs<MemBackend>>, env: ifs::auth::MemEnv, owner: u64) -> LocalIfs<MemBackend> {
    LocalIfs {
        ifs: Arc::clone(ifs),
        env: Arc::new(env),
        audit: Arc::new(MemAudit::default()),
        principal: user("QSECOFR"),
        owner: Owner(owner),
        job_ccsid: 37,
    }
}

fn shared() -> Arc<Ifs<MemBackend>> {
    let s = sys();
    Arc::new(s.ifs)
}

/// `BOB` (uid 100) owns `/home/bob` `*RWX` for himself only; `ANN` is in
/// `DEV`, which has `*RX` there.
fn homes(c: &dyn IfsClient) {
    c.mkdir("/home/bob", 0o750).unwrap();
    c.chgown("/home/bob", "BOB", true).unwrap();
    c.chgpgp("/home/bob", Some("DEV"), true).unwrap();
}

#[test]
fn calls_as_a_uid_have_its_authority_and_its_groups() {
    let ifs = shared();
    let env = env().principal(user("ANN"));
    let j = job(&ifs, env, 1);
    homes(&j);
    let bob = j.as_caller(&Caller::Uid { uid: 100 }).unwrap();
    let ann = j.as_caller(&Caller::Uid { uid: 101 }).unwrap();
    let zed = j
        .as_caller(&Caller::Profile { name: "zed".into() })
        .unwrap();
    // BOB makes a file; it is his.
    let fd = bob
        .open("/home/bob/a", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o640))
        .unwrap();
    bob.write(fd, b"hi").unwrap();
    bob.close(fd).unwrap();
    assert_eq!(bob.stat("/home/bob/a", true).unwrap().owner, "BOB");
    // ANN reads through her group, may not write; ZED may not search.
    assert_eq!(ann.access("/home/bob/a", R_OK), Ok(()));
    assert_eq!(ann.access("/home/bob/a", W_OK), Err(Errno::EACCES));
    assert_eq!(ann.read_dir("/home/bob").unwrap().len(), 1);
    assert_eq!(zed.stat("/home/bob/a", true), Err(Errno::EACCES));
    assert_eq!(ann.mkdir("/home/bob/d", 0o755), Err(Errno::EACCES));
    // Every other call as the user.
    bob.mkdir("/home/bob/d", 0o755).unwrap();
    bob.symlink("a", "/home/bob/l").unwrap();
    assert_eq!(bob.readlink("/home/bob/l").unwrap(), "a");
    bob.link("/home/bob/a", "/home/bob/b").unwrap();
    bob.rename("/home/bob/b", "/home/bob/d/b", RenameMode::Unlink)
        .unwrap();
    bob.chmod("/home/bob/a", 0o644).unwrap();
    assert_eq!(ann.chmod("/home/bob/a", 0o666), Err(Errno::EPERM));
    bob.utime("/home/bob/a", Some((1_000_000_000, 2_000_000_000)))
        .unwrap();
    assert_eq!(ann.chown("/home/bob/a", 101, 0), Err(Errno::EPERM));
    assert_eq!(bob.chown("/home/bob/a", 101, 0), Err(Errno::EPERM));
    j.chown("/home/bob/a", 101, 0).unwrap();
    assert_eq!(ann.stat("/home/bob/a", true).unwrap().owner, "ANN");
    assert!(!bob.get_attrs("/home/bob/a", &[], true).unwrap().is_empty());
    bob.statvfs("/home/bob").unwrap();
    bob.pathconf("/home/bob", 4).unwrap();
    let h = bob.opendir("/home/bob/d").unwrap();
    assert_eq!(bob.readdir(h, 10).unwrap().len(), 3, ". .. b");
    bob.closedir(h).unwrap();
    // b is a link to a, which ANN owns now.
    assert_eq!(bob.unlink("/home/bob/d/b"), Err(Errno::EACCES));
    j.unlink("/home/bob/d/b").unwrap();
    bob.rmdir("/home/bob/d").unwrap();
    assert_eq!(ann.unlink("/home/bob/l"), Err(Errno::EACCES));
    // A UID no profile has, and a name no profile has.
    assert_eq!(
        j.as_caller(&Caller::Uid { uid: 4242 }).err(),
        Some(Errno::ENOTENROLL)
    );
    assert_eq!(
        j.as_caller(&Caller::Profile {
            name: "NOBODY".into()
        })
        .err(),
        Some(Errno::ENOTENROLL)
    );
}

#[test]
fn descriptors_opened_as_a_user_are_the_jobs() {
    let ifs = shared();
    let j = job(&ifs, env(), 7);
    homes(&j);
    let bob = j.as_caller(&Caller::Uid { uid: 100 }).unwrap();
    let fd = bob
        .open("/home/bob/f", OpenArgs::new(O_RDWR | O_CREAT).mode(0o600))
        .unwrap();
    // The job's own client reaches the descriptor: the job keeps it.
    j.write(fd, b"x").unwrap();
    assert_eq!(ifs.owners(), vec![Owner(7)]);
    j.release();
    assert_eq!(ifs.open_count(), 0);
}

#[test]
fn audit_entries_name_the_user_called_as() {
    let s = sys();
    let p = user("QSECOFR");
    let cx = s.cx(&p, 1);
    s.ifs.mkdir(&cx, "/home/bob", 0o700).unwrap();
    s.ifs.chgown(&cx, "/home/bob", "BOB", true).unwrap();
    s.audit.take();
    let d = ifs::client::Direct {
        ifs: &s.ifs,
        cx: &cx,
    };
    let zed = d.as_caller(&Caller::Uid { uid: 102 }).unwrap();
    assert_eq!(zed.stat("/home/bob/x", true), Err(Errno::EACCES));
    let ev = s.audit.take();
    assert!(
        ev.iter()
            .any(|e| e.kind == AuditKind::AuthFail && e.user == "ZED"),
        "{ev:?}"
    );
}

fn id_of(c: &dyn IfsClient, p: &str) -> FileId {
    let s = c.stat(p, false).unwrap();
    FileId {
        fs: s.vfs,
        ino: s.ino,
        generation: s.ino_gen,
    }
}

#[test]
fn file_ids_find_their_object_without_the_cache_and_go_stale() {
    let ifs = shared();
    let j = job(&ifs, env(), 1);
    j.mkdir("/home/a", 0o755).unwrap();
    j.mkdir("/home/a/b", 0o755).unwrap();
    let fd = j
        .open("/home/a/b/f", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    j.close(fd).unwrap();
    let id = id_of(&j, "/home/a/b/f");
    assert_ne!(id.generation, 0);
    let (p, s) = j.stat_by_id(id).unwrap();
    assert_eq!((p.as_str(), s.ino), ("/home/a/b/f", id.ino));
    // Forgotten (a restart): the walk finds it, and remembers what it passed.
    ifs.forget_ids();
    assert_eq!(j.stat_by_id(id).unwrap().0, "/home/a/b/f");
    assert!(ifs.id_cache_len() > 3);
    // Renamed: still found.
    j.rename("/home/a/b", "/home/a/c", RenameMode::Keep)
        .unwrap();
    assert_eq!(j.stat_by_id(id).unwrap().0, "/home/a/c/f");
    let (fd, p) = j.open_by_id(id, OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(p, "/home/a/c/f");
    j.close(fd).unwrap();
    // Another generation, or gone: stale.
    let other = FileId {
        generation: id.generation + 1,
        ..id
    };
    assert_eq!(j.stat_by_id(other).err(), Some(Errno::ESTALE));
    j.unlink("/home/a/c/f").unwrap();
    assert_eq!(j.stat_by_id(id).err(), Some(Errno::ESTALE));
    assert_eq!(
        j.open_by_id(id, OpenArgs::new(O_RDONLY)).err(),
        Some(Errno::ESTALE)
    );
    // A file system that is not mounted.
    assert_eq!(
        j.stat_by_id(FileId { fs: 999, ..id }).err(),
        Some(Errno::ESTALE)
    );
    // Qp0lGetPathFromFileID keeps its answers.
    let d = id_of(&j, "/home/a/c");
    assert_eq!(
        j.path_from_file_id(FileId { generation: 0, ..d }).unwrap(),
        "/home/a/c"
    );
    assert_eq!(j.path_from_file_id(id), Err(Errno::ENOENT));
    // The caller must reach it.
    j.chmod("/home/a", 0o700).unwrap();
    let zed = j.as_caller(&Caller::Uid { uid: 102 }).unwrap();
    assert_eq!(zed.stat_by_id(d).err(), Some(Errno::EACCES));
}

#[test]
fn generations_survive_a_restart() {
    let b = MemBackend::new();
    let e = env();
    let q = || Box::new(MemQsys::new(Auth::USE));
    let first = Ifs::new(b.clone(), q(), Config::default());
    first.ensure_layout(&e).unwrap();
    let p = user("QSECOFR");
    let audit = MemAudit::default();
    let cx = ifs::Ctx {
        owner: Owner(1),
        principal: &p,
        env: &e,
        audit: &audit,
        job_ccsid: 37,
    };
    first.mkdir(&cx, "/home/x", 0o755).unwrap();
    let g1 = first.stat(&cx, "/home/x").unwrap().ino_gen;
    // The same volume after an IPL.
    let again = Ifs::new(b, q(), Config::default());
    again.mkdir(&cx, "/home/y", 0o755).unwrap();
    let g2 = again.stat(&cx, "/home/y").unwrap().ino_gen;
    assert!(g2 > g1 + ifs::meta::generation::BLOCK - 1, "{g1} {g2}");
    // An old file ID still finds its object.
    let st = again.stat(&cx, "/home/x").unwrap();
    let id = FileId {
        fs: st.vfs,
        ino: st.ino,
        generation: g1,
    };
    assert_eq!(again.stat_by_id(&cx, id).unwrap().0, "/home/x");
}

#[test]
fn qsys_file_ids_resolve_by_their_names() {
    let ifs = shared();
    let j = job(&ifs, env(), 1);
    for p in [
        "/QSYS.LIB/QGPL.LIB",
        "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE",
        "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR",
        "/QSYS.LIB/QSYSOPR.MSGQ",
        "/QSYS.LIB",
    ] {
        let id = id_of(&j, p);
        assert_eq!(id.fs, ids::QSYS);
        ifs.forget_ids();
        assert_eq!(j.stat_by_id(id).unwrap().0, p);
    }
    let gone = FileId {
        fs: ids::QSYS,
        ino: 1 << 63 | 0x3039,
        generation: 0,
    };
    assert_eq!(j.stat_by_id(gone).err(), Some(Errno::ESTALE));
    // Views that cannot be searched by ID.
    assert_eq!(
        j.stat_by_id(FileId {
            fs: ids::DEV,
            ino: 99,
            generation: 0
        })
        .err(),
        Some(Errno::ENOTSUP)
    );
}

fn open_rw(c: &dyn IfsClient, p: &str) -> i32 {
    c.open(p, OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap()
}

#[test]
fn sub_owners_hold_locks_that_conflict() {
    let ifs = shared();
    let j = job(&ifs, env(), 1);
    let other = job(&ifs, env(), 2);
    let fd = open_rw(&j, "/tmp/l");
    let w = RangeKind::Write;
    assert_eq!(
        j.lock_range_as(fd, 10, F_SETLK, w, SEEK_SET, 0, 10),
        Ok(None)
    );
    // Another client of the same job, and the job itself, conflict.
    assert_eq!(
        j.lock_range_as(fd, 11, F_SETLK, w, SEEK_SET, 5, 1),
        Err(Errno::EAGAIN)
    );
    let got = j
        .lock_range_as(fd, 11, F_GETLK, w, SEEK_SET, 5, 1)
        .unwrap()
        .unwrap();
    assert_eq!((got.owner, got.sub), (Owner(1), 10));
    assert_eq!(
        j.lock_range(fd, F_SETLK, w, SEEK_SET, 5, 1),
        Err(Errno::EAGAIN)
    );
    // Another job conflicts too.
    let fd2 = open_rw(&other, "/tmp/l");
    assert_eq!(
        other.lock_range(fd2, F_SETLK, w, SEEK_SET, 0, 1),
        Err(Errno::EAGAIN)
    );
    // Closing a descriptor leaves sub-owners' locks.
    let fd3 = open_rw(&j, "/tmp/l");
    j.close(fd3).unwrap();
    let held = j.owner_locks().unwrap();
    assert!(
        held.iter()
            .any(|(_, l)| matches!(l.what, Held::Range(r) if r.sub == 10))
    );
    // The client went: its locks go.
    assert_eq!(j.lock_release_sub(10), Ok(1));
    assert_eq!(j.lock_release_sub(0), Err(Errno::EINVAL));
    assert_eq!(other.lock_range(fd2, F_SETLKW, w, SEEK_SET, 0, 1), Ok(None));
}

#[test]
fn queued_locks_are_granted_through_the_poll() {
    let ifs = shared();
    let local = job(&ifs, env(), 1);
    let server = job(&ifs, env(), 2);
    let fd = open_rw(&local, "/tmp/q");
    let sfd = open_rw(&server, "/tmp/q");
    let w = RangeKind::Write;
    local.lock_range(fd, F_SETLK, w, SEEK_SET, 0, 0).unwrap();
    let t1 = server
        .lock_wait(sfd, 5, w, SEEK_SET, 0, 100)
        .unwrap()
        .unwrap();
    let t2 = server
        .lock_wait(sfd, 6, RangeKind::Read, SEEK_SET, 50, 1)
        .unwrap()
        .unwrap();
    assert_ne!(t1, t2);
    // Nothing granted yet: the poll would wait.
    let (r, d) = deferring(true, || server.with(|c| c.lease_orders(1000)));
    assert_eq!(r, Err(Errno::EAGAIN));
    let d = d.unwrap();
    assert!(!ifs.leases().ready(&d));
    assert!(server.lock_cancel(t2).unwrap());
    assert!(!server.lock_cancel(t2).unwrap());
    // The local job closes: its lock goes, the first request is granted.
    local.close(fd).unwrap();
    assert!(ifs.leases().ready(&d));
    let news = server.lease_orders(0).unwrap();
    assert_eq!(news.locks, vec![t1]);
    // Held now: the local job cannot lock it.
    let fd = open_rw(&local, "/tmp/q");
    assert_eq!(
        local.lock_range(fd, F_SETLK, w, SEEK_SET, 10, 1),
        Err(Errno::EAGAIN)
    );
    // A request granted at once answers no token.
    assert_eq!(server.lock_wait(sfd, 5, w, SEEK_SET, 200, 1), Ok(None));
    // Ending the server job ends its locks and its queue.
    let t3 = local.lock_wait(fd, 0, w, SEEK_SET, 0, 1).unwrap().unwrap();
    server.release();
    assert_eq!(local.lease_orders(0).unwrap().locks, vec![t3]);
    let _ = Duration::ZERO;
}

#[test]
fn share_reservations_hold_between_jobs() {
    let ifs = shared();
    let a = job(&ifs, env(), 1);
    let b = job(&ifs, env(), 2);
    let fd = a
        .open("/tmp/s", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    a.close(fd).unwrap();
    // NFSv4 OPEN: access read, write or both; deny none, read, write or both.
    let access = [O_RDONLY, O_WRONLY, O_RDWR];
    let deny = [O_SHARE_RDWR, O_SHARE_WRONLY, O_SHARE_RDONLY, O_SHARE_NONE];
    let reads = |f: u32| f != O_WRONLY;
    let writes = |f: u32| f != O_RDONLY;
    let lets_read = |s: u32| s == O_SHARE_RDWR || s == O_SHARE_RDONLY;
    let lets_write = |s: u32| s == O_SHARE_RDWR || s == O_SHARE_WRONLY;
    for &a1 in &access {
        for &d1 in &deny {
            for &a2 in &access {
                for &d2 in &deny {
                    let first = a.open("/tmp/s", OpenArgs::new(a1 | d1)).unwrap();
                    let second = b.open("/tmp/s", OpenArgs::new(a2 | d2));
                    let conflict = (reads(a1) && !lets_read(d2))
                        || (writes(a1) && !lets_write(d2))
                        || (reads(a2) && !lets_read(d1))
                        || (writes(a2) && !lets_write(d1));
                    assert_eq!(
                        second.is_err(),
                        conflict,
                        "{a1:#x} {d1:#x} then {a2:#x} {d2:#x}"
                    );
                    if let Ok(fd) = second {
                        b.close(fd).unwrap();
                    } else {
                        assert_eq!(second, Err(Errno::EBUSY));
                    }
                    a.close(first).unwrap();
                }
            }
        }
    }
}

/// A sync asked by a request that may wait is made on the syncer's
/// thread: the request defers until it is done, its next run finds it
/// done and does not write again; outside such a request the sync is
/// made in place. Either way the backend syncs once per call.
#[test]
#[allow(clippy::many_single_char_names)]
fn syncs_wait_off_the_callers_thread() {
    let mem = MemBackend::new();
    mem.set_sync_handles(true);
    let env = env();
    let ifs = Arc::new(Ifs::new(
        mem.clone(),
        Box::new(MemQsys::new(Auth::USE)),
        Config::default(),
    ));
    ifs.ensure_layout(&env).unwrap();
    let j = job(&ifs, env, 1);
    let fd = j
        .open("/tmp/s", OpenArgs::new(O_RDWR | O_CREAT).mode(0o600))
        .unwrap();
    // In place: no request.
    j.with(|c| c.fsync(fd, false)).unwrap();
    assert_eq!(mem.syncs(), 1);
    // A request that may wait defers, and its next run takes the result.
    let (r, d) = deferring(true, || {
        j.with(|c| c.pwrite_stat(fd, b"abc", 0, Some(false)))
    });
    assert_eq!(r, Err(Errno::EAGAIN));
    let d = d.expect("the sync defers the request");
    assert!(ifs.wait_deferral(&d));
    assert_eq!(mem.syncs(), 2);
    let t = ifs::remote::request::last().expect("its token");
    // Written meanwhile by another call: the run does not write again.
    j.pwrite(fd, b"xyz", 0).unwrap();
    ifs::remote::request::rerun(t);
    let (r, d) = deferring(true, || {
        j.with(|c| c.pwrite_stat(fd, b"abc", 0, Some(false)))
    });
    assert!(d.is_none());
    let (n, st) = r.unwrap();
    assert_eq!((n, st.map(|s| s.size)), (3, Some(3)));
    assert_eq!(j.pread(fd, 3, 0).unwrap(), b"xyz");
    assert_eq!(mem.syncs(), 2, "the run took the sync made");
    // Through the client that waits: one sync per call.
    assert_eq!(j.pwrite_stat(fd, b"def", 3, Some(true)).unwrap().0, 3);
    j.fsync(fd, false).unwrap();
    assert_eq!(mem.syncs(), 4);
    j.close(fd).unwrap();
}
