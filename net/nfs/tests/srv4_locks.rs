// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4 byte-range locks: two v4 clients, a v4 client and an NLM
//! client (the lock manager of another job over the same file system),
//! a local job; `LOCKT`, `LOCKU`, upgrades, `RELEASE_LOCKOWNER`, `CLOSE`
//! with locks held, and seqid replays of `LOCK`.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use std::sync::Arc;
use std::time::Duration;

use ifs::client::IfsClient;
use ifs::lock::range::RangeKind;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{F_SETLK, O_RDWR, SEEK_SET};
use nfs::nlm::Nlm;
use nfs::nlm::state::{NlmConfig, NlmState};
use nfs::rpc::{ClientConfig, Dispatcher};
use nfs::server::ifsvfs::{self, IfsVfs};
use nfs::server::locate::FixedLocator;
use nfs::server::{Clock, SystemClock};
use nfs::types::nfs4::{self as t, nfs_argop4, nfs_lock_type4, nfs_resop4, nfsstat4};
use nfs::types::nlm::NLM_PROG::NLM4_VERS;
use nfs::types::nlm::{netobj, nlm4_lock, nlm4_lockargs, nlm4_stats};
use srv3_common::{Client, World};
use srv4_common::{Env, V4c, close, fh_of, getfh, lookup, opened, putfh, rootfh};

fn env() -> Env {
    let w = World::new();
    let e = w.export(1, "/home/x");
    w.put(e);
    w.write_file("/home/x/f", b"0123456789");
    w.job(9).chmod("/home/x/f", 0o666).unwrap();
    Env::new(w)
}

/// A client with the file open for reading and writing: its handle and
/// open stateid.
fn opener(e: &Env, peer: &str, id: &str) -> (V4c, Vec<u8>, t::stateid4) {
    let c = V4c::new(peer, 100, 0);
    c.setclientid(e, id, "127.0.0.1.0.1");
    let r = c.raw(
        e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", "f", false, 3, 0),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let o = opened(&r);
    let fh = fh_of(&r);
    let r = c.raw(
        e,
        0,
        vec![
            putfh(&fh),
            nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                open_stateid: o.stateid.clone(),
                seqid: c.next_oseq(),
            }),
        ],
    );
    let nfs_resop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4res::NFS4_OK(ok)) = &r.resarray[1] else {
        panic!("{r:?}")
    };
    (c, fh, ok.open_stateid.clone())
}

fn new_lock(c: &V4c, sid: &t::stateid4, kind: nfs_lock_type4, off: u64, len: u64) -> nfs_argop4 {
    nfs_argop4::OP_LOCK(t::LOCK4args {
        locktype: kind,
        reclaim: false,
        offset: off,
        length: len,
        locker: t::locker4::TRUE(t::open_to_lock_owner4 {
            open_seqid: c.next_oseq(),
            open_stateid: sid.clone(),
            lock_seqid: 0,
            lock_owner: t::state_owner4 {
                clientid: c.clientid.get(),
                owner: b"lo".to_vec(),
            },
        }),
    })
}

fn more_lock(lsid: &t::stateid4, seq: u32, kind: nfs_lock_type4, off: u64, len: u64) -> nfs_argop4 {
    nfs_argop4::OP_LOCK(t::LOCK4args {
        locktype: kind,
        reclaim: false,
        offset: off,
        length: len,
        locker: t::locker4::FALSE(t::exist_lock_owner4 {
            lock_stateid: lsid.clone(),
            lock_seqid: seq,
        }),
    })
}

fn locked(r: &t::COMPOUND4res) -> t::stateid4 {
    match r.resarray.last().unwrap() {
        nfs_resop4::OP_LOCK(t::LOCK4res::NFS4_OK(ok)) => ok.lock_stateid.clone(),
        x => panic!("{x:?}"),
    }
}

fn denied(r: &t::COMPOUND4res) -> t::LOCK4denied {
    match r.resarray.last().unwrap() {
        nfs_resop4::OP_LOCK(t::LOCK4res::NFS4ERR_DENIED(d))
        | nfs_resop4::OP_LOCKT(t::LOCKT4res::NFS4ERR_DENIED(d)) => d.clone(),
        x => panic!("{x:?}"),
    }
}

#[test]
fn two_v4_clients() {
    let e = env();
    let (a, fh, sa) = opener(&e, "10.0.0.1:900", "lock-a");
    let (b, _, sb) = opener(&e, "10.0.0.2:900", "lock-b");
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&a, &sa, nfs_lock_type4::WRITE_LT, 0, 10),
        ],
    );
    let la = locked(&r);
    assert_eq!(la.seqid, 1);
    // b is denied, told who holds it.
    let r = b.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&b, &sb, nfs_lock_type4::WRITEW_LT, 5, 1),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_DENIED);
    let d = denied(&r);
    assert_eq!((d.offset, d.length), (0, 10));
    assert_eq!(d.owner.clientid, a.clientid.get());
    assert_eq!(d.owner.owner, b"lo");
    // LOCKT says the same; beyond the range is free.
    let lockt = |off, len| {
        nfs_argop4::OP_LOCKT(t::LOCKT4args {
            locktype: nfs_lock_type4::READ_LT,
            offset: off,
            length: len,
            owner: t::state_owner4 {
                clientid: b.clientid.get(),
                owner: b"lo".to_vec(),
            },
        })
    };
    assert_eq!(
        b.raw(&e, 0, vec![putfh(&fh), lockt(9, 1)]).status,
        nfsstat4::NFS4ERR_DENIED
    );
    assert_eq!(
        b.raw(&e, 0, vec![putfh(&fh), lockt(10, u64::MAX)]).status,
        nfsstat4::NFS4_OK
    );
    // A length of 0, and a range past the end of the offsets.
    assert_eq!(
        b.raw(&e, 0, vec![putfh(&fh), lockt(1, 0)]).status,
        nfsstat4::NFS4ERR_INVAL
    );
    assert_eq!(
        b.raw(&e, 0, vec![putfh(&fh), lockt(u64::MAX - 1, 5)])
            .status,
        nfsstat4::NFS4ERR_INVAL
    );
    // a downgrades part of its lock and unlocks the rest.
    let r = a.raw(
        &e,
        0,
        vec![putfh(&fh), more_lock(&la, 1, nfs_lock_type4::READ_LT, 0, 5)],
    );
    let la = locked(&r);
    assert_eq!(la.seqid, 2);
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            nfs_argop4::OP_LOCKU(t::LOCKU4args {
                locktype: nfs_lock_type4::WRITE_LT,
                seqid: 2,
                lock_stateid: la.clone(),
                offset: 5,
                length: 5,
            }),
        ],
    );
    let nfs_resop4::OP_LOCKU(t::LOCKU4res::NFS4_OK(la)) = r.resarray[1].clone() else {
        panic!("{r:?}")
    };
    // Now b may write-lock 5..10 but not 0..5 (a still reads it).
    let r = b.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&b, &sb, nfs_lock_type4::WRITE_LT, 5, 5),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    // CLOSE with locks held is refused in 4.0; RELEASE_LOCKOWNER too.
    let r = a.raw(&e, 0, vec![putfh(&fh), close(a.next_oseq(), &sa)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_LOCKS_HELD);
    let rel = nfs_argop4::OP_RELEASE_LOCKOWNER(t::RELEASE_LOCKOWNER4args {
        lock_owner: t::state_owner4 {
            clientid: a.clientid.get(),
            owner: b"lo".to_vec(),
        },
    });
    assert_eq!(
        a.raw(&e, 0, vec![rel.clone()]).status,
        nfsstat4::NFS4ERR_LOCKS_HELD
    );
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            nfs_argop4::OP_LOCKU(t::LOCKU4args {
                locktype: nfs_lock_type4::READ_LT,
                seqid: 3,
                lock_stateid: la,
                offset: 0,
                length: u64::MAX,
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert_eq!(a.raw(&e, 0, vec![rel]).status, nfsstat4::NFS4_OK);
}

#[test]
fn lock_retransmission_and_bad_seqid() {
    let e = env();
    let (a, fh, sa) = opener(&e, "10.0.0.1:900", "lock-r");
    let op = new_lock(&a, &sa, nfs_lock_type4::READ_LT, 0, 1);
    let r1 = a.raw(&e, 0, vec![putfh(&fh), op.clone()]);
    let r2 = a.raw(&e, 0, vec![putfh(&fh), op]);
    assert_eq!(r1, r2);
    let la = locked(&r1);
    let r = a.raw(
        &e,
        0,
        vec![putfh(&fh), more_lock(&la, 7, nfs_lock_type4::READ_LT, 0, 1)],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_SEQID);
    // A write lock through a read-only open is the wrong mode.
    let c = V4c::new("10.0.0.3:900", 100, 0);
    c.setclientid(&e, "lock-ro", "127.0.0.1.0.1");
    let r = c.raw(
        &e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", "f", false, 1, 0),
        ],
    );
    let o = opened(&r);
    c.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                open_stateid: o.stateid.clone(),
                seqid: c.next_oseq(),
            }),
        ],
    );
    let mut s = o.stateid;
    s.seqid += 1;
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&c, &s, nfs_lock_type4::WRITE_LT, 50, 1),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_OPENMODE);
}

#[test]
fn a_local_job_and_an_nlm_client() {
    let e = env();
    let (a, fh, sa) = opener(&e, "10.0.0.1:900", "lock-l");
    // A local job holds 20..30.
    let j = e.w.job(5);
    let fd = j.open("/home/x/f", OpenArgs::new(O_RDWR)).unwrap();
    j.lock_range_as(fd, 0, F_SETLK, RangeKind::Write, SEEK_SET, 20, 10)
        .unwrap();
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&a, &sa, nfs_lock_type4::READ_LT, 25, 1),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_DENIED);
    assert_eq!(denied(&r).owner.clientid, 0, "not a v4 owner");
    j.close(fd).unwrap();
    // The v4 client takes 0..10; an NLM client of the lock manager's job
    // is refused it, and gets it once released.
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            new_lock(&a, &sa, nfs_lock_type4::WRITE_LT, 0, 10),
        ],
    );
    let la = locked(&r);
    let st = NlmState::new(NlmConfig {
        monitor: false,
        locator: Arc::new(FixedLocator::new()),
        rpc: ClientConfig::default(),
        grace: Duration::ZERO,
        grant_tries: 1,
        clock: Arc::new(SystemClock) as Arc<dyn Clock>,
    });
    let nlm_vfs = IfsVfs::new(
        e.w.job(2),
        Arc::clone(&e.w.instance),
        ifsvfs::Config::default(),
    );
    let p = Nlm::new(nlm_vfs, e.w.gate(), st);
    let mut c = Client::new("10.0.0.5:700", 100);
    c.d = Dispatcher::new();
    let nlock = |off, len| {
        NLM4_VERS::Args::NLMPROC4_LOCK(nlm4_lockargs {
            cookie: netobj(b"c".to_vec()),
            block: false,
            exclusive: true,
            alock: nlm4_lock {
                caller_name: b"nlmhost".to_vec(),
                fh: netobj(fh.clone()),
                oh: netobj(b"oh".to_vec()),
                svid: 3,
                l_offset: off,
                l_len: len,
            },
            reclaim: false,
            state: 1,
        })
    };
    let stat = |r: NLM4_VERS::Res| match r {
        NLM4_VERS::Res::NLMPROC4_LOCK(x) => x.stat.stat,
        r => panic!("{r:?}"),
    };
    assert_eq!(stat(c.call(&p, &nlock(5, 1))), nlm4_stats::NLM4_DENIED);
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&fh),
            nfs_argop4::OP_LOCKU(t::LOCKU4args {
                locktype: nfs_lock_type4::WRITE_LT,
                seqid: 1,
                lock_stateid: la,
                offset: 0,
                length: u64::MAX,
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let nfs_resop4::OP_LOCKU(t::LOCKU4res::NFS4_OK(la)) = r.resarray[1].clone() else {
        panic!("{r:?}")
    };
    assert_eq!(stat(c.call(&p, &nlock(5, 1))), nlm4_stats::NLM4_GRANTED);
    // And the v4 client is refused what the NLM client holds.
    let r = a.raw(
        &e,
        0,
        vec![putfh(&fh), more_lock(&la, 2, nfs_lock_type4::READ_LT, 5, 1)],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_DENIED);
}
