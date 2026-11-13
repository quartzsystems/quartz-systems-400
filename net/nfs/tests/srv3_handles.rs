// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Handles over time: stale after a remove and a new file of the same
//! name, good after a restart of the server and the system, checked
//! against the export table on every call; changes across exports;
//! retransmissions (the duplicate request cache, exclusive create);
//! the write verifier across a restart.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use nfs::server::FileHandle;
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3, nfsstat3 as S};
use srv3_common::{Client, V3, World, dirop, fh};

const PEER: &str = "10.0.0.5:900";

fn world() -> (World, Vec<u8>) {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let root = w.handle(1, "/home/exp");
    (w, root)
}

#[test]
fn a_new_file_of_the_same_name_has_another_handle() {
    let (w, root) = world();
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let a = v.create(&root, "f").unwrap();
    v.write(&a, 0, b"one", t3::stable_how::FILE_SYNC).unwrap();
    v.remove(&root, "f").unwrap();
    let b = v.create(&root, "f").unwrap();
    assert_ne!(a, b);
    assert_eq!(v.getattr(&a), Err(S::NFS3ERR_STALE));
    assert_eq!(v.read(&a, 0, 9), Err(S::NFS3ERR_STALE));
    assert_eq!(v.read(&b, 0, 9).unwrap().0, b"");
}

#[test]
fn handles_outlive_a_restart() {
    let (w, root) = world();
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let d = v.mkdir(&root, "d").unwrap();
    let f = v.create(&d, "f").unwrap();
    let before = v.write(&f, 0, b"kept", t3::stable_how::UNSTABLE).unwrap();
    drop(p);
    let again = w.restarted();
    let p2 = again.nfs3();
    let v2 = V3 { p: &p2, c: &c };
    // The system forgot every path: the handle is found by its file ID.
    assert_eq!(v2.read(&f, 0, 10).unwrap().0, b"kept");
    assert_eq!(v2.lookup(&d, "f"), Ok(f.clone()));
    // A new start: a new write verifier, so the client writes again.
    let after = v2.write(&f, 0, b"KEPT", t3::stable_how::UNSTABLE).unwrap();
    assert_ne!(before.verf, after.verf);
    match c.v3(
        &p2,
        &Args::NFSPROC3_COMMIT(t3::COMMIT3args {
            file: fh(&f),
            offset: 0,
            count: 0,
        }),
    ) {
        Res::NFSPROC3_COMMIT(t3::COMMIT3res::NFS3_OK(ok)) => assert_eq!(ok.verf, after.verf),
        r => panic!("{r:?}"),
    }
}

#[test]
fn the_export_table_is_checked_on_every_call() {
    let (w, root) = world();
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let f = v.create(&root, "f").unwrap();
    // Unexported: stale at once.
    w.exports.remove(1);
    assert_eq!(v.getattr(&f), Err(S::NFS3ERR_STALE));
    // Exported again with the same ID over another directory: still stale.
    let other = w.export(1, "/home/other");
    w.put(other);
    assert_eq!(v.getattr(&f), Err(S::NFS3ERR_STALE));
    // The same directory again: good.
    let e = w.export(1, "/home/exp");
    w.put(e);
    assert!(v.getattr(&f).is_ok());
    // A handle naming export 1 for an object outside it.
    let e2 = w.export(2, "/home/other");
    w.put(e2);
    w.write_file("/home/other/g", b"g");
    let mut forged = FileHandle::from_bytes(&w.handle(2, "/home/other/g")).unwrap();
    forged.export = 1;
    forged.root_ino = w.fid("/home/exp").ino;
    assert_eq!(v.getattr(&forged.to_bytes()), Err(S::NFS3ERR_BADHANDLE));
    // Not a handle at all.
    assert_eq!(v.getattr(&[1, 2, 3]), Err(S::NFS3ERR_BADHANDLE));
    let mut bent = f.clone();
    bent[30] ^= 0xff;
    assert_eq!(v.getattr(&bent), Err(S::NFS3ERR_BADHANDLE));
}

#[test]
fn rename_and_link_across_exports_are_xdev() {
    let (w, root) = world();
    let e2 = w.export(2, "/home/two");
    w.put(e2);
    let two = w.handle(2, "/home/two");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let f = v.create(&root, "f").unwrap();
    let a = t3::RENAME3args {
        from: dirop(&root, "f"),
        to: dirop(&two, "f"),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_RENAME(a)),
        Res::NFSPROC3_RENAME(t3::RENAME3res::default(S::NFS3ERR_XDEV, _))
    ));
    let a = t3::LINK3args {
        file: fh(&f),
        link: dirop(&two, "f"),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_LINK(a)),
        Res::NFSPROC3_LINK(t3::LINK3res::default(S::NFS3ERR_XDEV, _))
    ));
}

#[test]
fn a_retransmitted_rename_gets_the_kept_reply() {
    let (w, root) = world();
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    v.create(&root, "a").unwrap();
    let args = Args::NFSPROC3_RENAME(t3::RENAME3args {
        from: dirop(&root, "a"),
        to: dirop(&root, "b"),
    });
    let first: Res = c.call_xid(&p, 4711, &args);
    assert!(matches!(
        first,
        Res::NFSPROC3_RENAME(t3::RENAME3res::NFS3_OK(_))
    ));
    // The same XID again: the first reply, not NOENT.
    let again: Res = c.call_xid(&p, 4711, &args);
    assert_eq!(again, first);
    // A new XID runs it again.
    let fresh: Res = c.call_xid(&p, 4712, &args);
    assert!(matches!(
        fresh,
        Res::NFSPROC3_RENAME(t3::RENAME3res::default(S::NFS3ERR_NOENT, _))
    ));
    // Idempotent procedures are not kept.
    assert!(
        c.d.handle(&c.message(9, &Args::NFSPROC3_NULL), c.peer, c.transport)
            .is_some()
    );
}

#[test]
fn an_exclusive_create_is_made_once() {
    let (w, root) = world();
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let verf = *b"\x12\x34\x56\x78\x9a\xbc\xde\xf0";
    let how = t3::createhow3::EXCLUSIVE(verf);
    let a = v.create_how(&root, "x", how.clone()).unwrap();
    // A retransmission past the cache (another XID): the same file.
    assert_eq!(v.create_how(&root, "x", how), Ok(a.clone()));
    // Another verifier: it exists.
    assert_eq!(
        v.create_how(&root, "x", t3::createhow3::EXCLUSIVE([9; 8])),
        Err(S::NFS3ERR_EXIST)
    );
    // After a restart too.
    let again = w.restarted();
    let p2 = again.nfs3();
    let v2 = V3 { p: &p2, c: &c };
    assert_eq!(
        v2.create_how(&root, "x", t3::createhow3::EXCLUSIVE(verf)),
        Ok(a)
    );
}

/// Data is on disk when the reply says so: an `UNSTABLE` write is not
/// synced (its verifier tells the client what a restart lost), `COMMIT`
/// and `FILE_SYNC` are; a `*SYNC` host's writes are all answered
/// `FILE_SYNC`, each synced before the reply. The syncs are made off the
/// caller's thread, as in the service.
#[test]
fn writes_are_synced_as_the_reply_says() {
    let (w, root) = world();
    w.backend.set_sync_handles(true);
    let mut e = w.export(2, "/home/sync");
    e.default_opts.sync = true;
    w.put(e);
    let sroot = w.handle(2, "/home/sync");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let f = v.create(&root, "f").unwrap();
    let synced = || w.backend.syncs();
    let base = synced();
    let r = v.write(&f, 0, b"one", t3::stable_how::UNSTABLE).unwrap();
    assert_eq!(r.committed, t3::stable_how::UNSTABLE);
    assert_eq!(synced(), base, "an unstable write is not synced");
    match c.v3(
        &p,
        &Args::NFSPROC3_COMMIT(t3::COMMIT3args {
            file: fh(&f),
            offset: 0,
            count: 0,
        }),
    ) {
        Res::NFSPROC3_COMMIT(t3::COMMIT3res::NFS3_OK(ok)) => assert_eq!(ok.verf, r.verf),
        r => panic!("{r:?}"),
    }
    assert_eq!(synced(), base + 1, "COMMIT syncs");
    let r = v.write(&f, 3, b"two", t3::stable_how::FILE_SYNC).unwrap();
    assert_eq!(r.committed, t3::stable_how::FILE_SYNC);
    assert_eq!(synced(), base + 2);
    // `*SYNC`: unstable asked, stable answered and made.
    let g = v.create(&sroot, "g").unwrap();
    let r = v.write(&g, 0, b"three", t3::stable_how::UNSTABLE).unwrap();
    assert_eq!(r.committed, t3::stable_how::FILE_SYNC);
    assert_eq!(synced(), base + 3);
    assert_eq!(v.read(&g, 0, 9).unwrap().0, b"three");
    assert_eq!(v.read(&f, 0, 9).unwrap().0, b"onetwo");
}
