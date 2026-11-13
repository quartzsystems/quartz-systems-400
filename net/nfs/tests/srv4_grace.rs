// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A restart: the stable records name the clients that held state; in
//! the grace period only they reclaim opens and locks, new state is
//! `NFS4ERR_GRACE`, a client the records do not name is
//! `NFS4ERR_NO_GRACE`; the period ends when every client named is done
//! (`RECLAIM_COMPLETE`) or its time is up.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use ifs::client::IfsClient;
use nfs::types::nfs4::{self as t, nfs_argop4, nfs_lock_type4, nfs_resop4, nfsstat4};
use nfs::v4::state::stable;
use srv3_common::World;
use srv4_common::{Env, V4c, fh_of, getfh, lookup, opened, putfh, rootfh};

fn env() -> Env {
    let w = World::new();
    let e = w.export(1, "/home/x");
    w.put(e);
    w.write_file("/home/x/f", b"0123456789");
    w.write_file("/home/x/g", b"0123456789");
    w.job(9).chmod("/home/x/f", 0o666).unwrap();
    w.job(9).chmod("/home/x/g", 0o666).unwrap();
    Env::new(w)
}

fn previous(seqid: u32, owner: t::open_owner4) -> nfs_argop4 {
    nfs_argop4::OP_OPEN(t::OPEN4args {
        seqid,
        share_access: 3,
        share_deny: 0,
        owner,
        openhow: t::openflag4::default(t::opentype4::OPEN4_NOCREATE),
        claim: t::open_claim4::CLAIM_PREVIOUS(t::open_delegation_type4::OPEN_DELEGATE_NONE),
    })
}

fn lock(c: &V4c, sid: &t::stateid4, reclaim: bool, off: u64) -> nfs_argop4 {
    nfs_argop4::OP_LOCK(t::LOCK4args {
        locktype: nfs_lock_type4::WRITE_LT,
        reclaim,
        offset: off,
        length: 10,
        locker: t::locker4::TRUE(t::open_to_lock_owner4 {
            open_seqid: c.next_oseq(),
            open_stateid: sid.clone(),
            lock_seqid: 0,
            lock_owner: t::state_owner4 {
                clientid: c.clientid.get(),
                owner: format!("lo{off}").into_bytes(),
            },
        }),
    })
}

fn confirm(e: &Env, c: &V4c, fh: &[u8], s: &t::stateid4) -> t::stateid4 {
    let r = c.raw(
        e,
        0,
        vec![
            putfh(fh),
            nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                open_stateid: s.clone(),
                seqid: c.next_oseq(),
            }),
        ],
    );
    let nfs_resop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4res::NFS4_OK(ok)) = &r.resarray[1] else {
        panic!("{r:?}")
    };
    ok.open_stateid.clone()
}

#[test]
fn reclaims_in_the_grace_period() {
    let e = env();
    assert!(!e.srv.in_grace(), "no records: no grace period");
    // A 4.0 client with an open and a lock, a 4.1 client with an open.
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e, "grace-a", "127.0.0.1.0.1");
    let r = a.raw(
        &e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            a.open("o", "f", false, 3, 0),
            getfh(),
        ],
    );
    let f = fh_of(&r);
    let sa = confirm(&e, &a, &f, &opened(&r).stateid);
    let r = a.raw(&e, 0, vec![putfh(&f), lock(&a, &sa, false, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let b = V4c::new("10.0.0.2:900", 100, 1);
    b.session(&e, "grace-b", 0);
    let r = b.run(
        &e,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            b.open("o", "g", false, 3, 0),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let g = fh_of(&r);
    let img = stable::decode(&e.stable.bytes().unwrap()).unwrap();
    assert_eq!(img.clients.len(), 2);
    assert!(img.clients.iter().all(|c| c.holds));
    drop(a);
    drop(b);

    // The server starts again.
    let e2 = e.restart();
    assert!(e2.srv.in_grace());
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e2, "grace-a", "127.0.0.1.0.1");
    // New state is refused; a reclaim is taken.
    let r = a.raw(
        &e2,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            a.open("o", "g", false, 1, 0),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_GRACE);
    let r = a.raw(
        &e2,
        0,
        vec![putfh(&f), previous(a.next_oseq(), a.owner("o"))],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let sa = confirm(&e2, &a, &f, &opened(&r).stateid);
    let r = a.raw(&e2, 0, vec![putfh(&f), lock(&a, &sa, true, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    // A client the records do not name has nothing to reclaim.
    let c = V4c::new("10.0.0.3:900", 100, 0);
    c.setclientid(&e2, "grace-c", "127.0.0.1.0.1");
    let r = c.raw(
        &e2,
        0,
        vec![putfh(&g), previous(c.next_oseq(), c.owner("o"))],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NO_GRACE);
    // A's first request for new state says it is done.
    let r = a.raw(&e2, 0, vec![putfh(&f), lock(&a, &sa, false, 50)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_GRACE);
    // B reclaims and completes: every client named is done, grace ends.
    let b = V4c::new("10.0.0.2:900", 100, 1);
    b.session(&e2, "grace-b", 0);
    let r = b.run(&e2, vec![putfh(&g), previous(0, b.owner("o"))]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert!(e2.srv.in_grace());
    let r = b.run(
        &e2,
        vec![nfs_argop4::OP_RECLAIM_COMPLETE(t::RECLAIM_COMPLETE4args {
            rca_one_fs: false,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    assert!(!e2.srv.in_grace());
    let r = b.run(&e2, vec![putfh(&g), previous(0, b.owner("o9"))]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_NO_GRACE);
    let r = c.raw(
        &e2,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", "f", false, 1, 0),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
}

#[test]
fn the_grace_period_runs_out() {
    let e = env();
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e, "late", "127.0.0.1.0.1");
    let r = a.raw(
        &e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            a.open("o", "f", false, 3, 0),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let f = fh_of(&r);
    let e2 = e.restart();
    assert!(e2.srv.in_grace());
    // Started again during the grace period: the client still may.
    let e3 = e2.restart();
    assert!(e3.srv.in_grace());
    e3.advance(91);
    e3.tick();
    assert!(!e3.srv.in_grace());
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e3, "late", "127.0.0.1.0.1");
    let r = a.raw(
        &e3,
        0,
        vec![putfh(&f), previous(a.next_oseq(), a.owner("o"))],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NO_GRACE);
    let img = stable::decode(&e3.stable.bytes().unwrap()).unwrap();
    assert!(img.grace_done);
    assert!(img.clients.is_empty(), "nobody holds state now");
}
