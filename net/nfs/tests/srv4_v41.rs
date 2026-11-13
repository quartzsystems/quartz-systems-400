// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4.1: `EXCHANGE_ID`'s cases, `CREATE_SESSION`'s sequence and
//! replay, slots (a retransmission gets the kept reply; misordered,
//! uncached, false retries, bad slots), the current stateid, `CLOSE`,
//! `TEST_STATEID`, `FREE_STATEID`, `DESTROY_SESSION`,
//! `DESTROY_CLIENTID`, `RECLAIM_COMPLETE`.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use ifs::client::IfsClient;

use nfs::types::nfs4::{
    self as t, EXCHGID4_FLAG_CONFIRMED_R, EXCHGID4_FLAG_UPD_CONFIRMED_REC_A,
    EXCHGID4_FLAG_USE_NON_PNFS, nfs_argop4, nfs_resop4, nfsstat4,
};
use srv3_common::World;
use srv4_common::{
    Env, V4c, create_session, exchange_id, fh_of, getfh, lookup, opened, putfh, read, rootfh,
    statuses, write,
};

fn env() -> Env {
    let w = World::new();
    let e = w.export(1, "/home/x");
    w.put(e);
    Env::new(w)
}

fn exid(r: &t::COMPOUND4res) -> t::EXCHANGE_ID4resok {
    match &r.resarray[0] {
        nfs_resop4::OP_EXCHANGE_ID(t::EXCHANGE_ID4res::NFS4_OK(ok)) => ok.clone(),
        x => panic!("{x:?}"),
    }
}

#[test]
fn exchange_id_cases() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    // Case 1: new; unconfirmed, sequence 1, never pNFS.
    let a = exid(&c.raw(&e, 1, vec![exchange_id("ex", [1; 8], 0)]));
    assert_eq!(a.eir_sequenceid, 1);
    assert_eq!(
        a.eir_flags & EXCHGID4_FLAG_USE_NON_PNFS,
        EXCHGID4_FLAG_USE_NON_PNFS
    );
    assert_eq!(a.eir_flags & EXCHGID4_FLAG_CONFIRMED_R, 0);
    // Case 4: the unconfirmed record is replaced.
    let b = exid(&c.raw(&e, 1, vec![exchange_id("ex", [1; 8], 0)]));
    assert_ne!(a.eir_clientid, b.eir_clientid);
    // The old one is gone.
    let r = c.raw(&e, 1, vec![create_session(a.eir_clientid, 1, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
    let r = c.raw(&e, 1, vec![create_session(b.eir_clientid, 1, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    // Case 2: confirmed, same verifier: the same ID.
    let b2 = exid(&c.raw(&e, 1, vec![exchange_id("ex", [1; 8], 0)]));
    assert_eq!(b2.eir_clientid, b.eir_clientid);
    assert_ne!(b2.eir_flags & EXCHGID4_FLAG_CONFIRMED_R, 0);
    // An update with another verifier, and by another principal.
    let r = c.raw(
        &e,
        1,
        vec![exchange_id("ex", [9; 8], EXCHGID4_FLAG_UPD_CONFIRMED_REC_A)],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOT_SAME);
    let other = V4c::new("10.0.0.1:901", 102, 1);
    let r = other.raw(
        &e,
        1,
        vec![exchange_id("ex", [1; 8], EXCHGID4_FLAG_UPD_CONFIRMED_REC_A)],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_PERM);
    let r = c.raw(
        &e,
        1,
        vec![exchange_id(
            "nobody",
            [1; 8],
            EXCHGID4_FLAG_UPD_CONFIRMED_REC_A,
        )],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOENT);
    // Case 5: a restart (new verifier) is a new ID.
    let d = exid(&c.raw(&e, 1, vec![exchange_id("ex", [7; 8], 0)]));
    assert_ne!(d.eir_clientid, b.eir_clientid);
    // Flags a client may not send.
    let r = c.raw(
        &e,
        1,
        vec![exchange_id("ex", [7; 8], EXCHGID4_FLAG_CONFIRMED_R)],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_INVAL);
}

#[test]
fn create_session_sequence_and_replay() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    let a = exid(&c.raw(&e, 1, vec![exchange_id("cs", [1; 8], 0)]));
    let r = c.raw(&e, 1, vec![create_session(a.eir_clientid, 2, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_SEQ_MISORDERED);
    let r1 = c.raw(&e, 1, vec![create_session(a.eir_clientid, 1, 0)]);
    assert_eq!(r1.status, nfsstat4::NFS4_OK);
    let r2 = c.raw(&e, 1, vec![create_session(a.eir_clientid, 1, 0)]);
    assert_eq!(r1, r2, "the kept reply");
    let nfs_resop4::OP_CREATE_SESSION(t::CREATE_SESSION4res::NFS4_OK(cs)) = &r1.resarray[0] else {
        panic!()
    };
    assert_eq!(cs.csr_fore_chan_attrs.ca_maxrequests, 4);
    assert_eq!(cs.csr_fore_chan_attrs.ca_maxresponsesize_cached, 8192);
    // Back channel asked, but this connection cannot carry one.
    assert_eq!(cs.csr_flags, 0);
    // Another principal.
    let o = V4c::new("10.0.0.1:901", 102, 1);
    let r = o.raw(&e, 1, vec![create_session(a.eir_clientid, 2, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_CLID_INUSE);
}

#[test]
fn slots_give_exactly_once_replies() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    let sid = c.session(&e, "slots", 0);
    let seq = |n: u32, slot: u32, cache: bool| {
        nfs_argop4::OP_SEQUENCE(t::SEQUENCE4args {
            sa_sessionid: sid,
            sa_sequenceid: n,
            sa_slotid: slot,
            sa_highest_slotid: 3,
            sa_cachethis: cache,
        })
    };
    let mk = nfs_argop4::OP_CREATE(t::CREATE4args {
        objtype: t::createtype4::NF4DIR,
        objname: t::utf8string(b"once".to_vec()),
        createattrs: srv4_common::no_attrs(),
    });
    let ops = || {
        vec![
            seq(1, 0, true),
            rootfh(),
            lookup("home"),
            lookup("x"),
            mk.clone(),
        ]
    };
    let r1 = c.raw(&e, 1, ops());
    assert_eq!(r1.status, nfsstat4::NFS4_OK, "{r1:?}");
    // A retransmission: the same reply, nothing done twice.
    let r2 = c.raw(&e, 1, ops());
    assert_eq!(r1, r2);
    // The same sequence ID with another request: a false retry.
    let r = c.raw(&e, 1, vec![seq(1, 0, true), rootfh()]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_SEQ_FALSE_RETRY);
    // Out of order, and a slot beyond the table.
    assert_eq!(
        c.raw(&e, 1, vec![seq(5, 0, false)]).status,
        nfsstat4::NFS4ERR_SEQ_MISORDERED
    );
    assert_eq!(
        c.raw(&e, 1, vec![seq(1, 9, false)]).status,
        nfsstat4::NFS4ERR_BADSLOT
    );
    // Not kept: the retransmission cannot be answered.
    let r = c.raw(&e, 1, vec![seq(1, 1, false), rootfh()]);
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let nfs_resop4::OP_SEQUENCE(t::SEQUENCE4res::NFS4_OK(s)) = &r.resarray[0] else {
        panic!()
    };
    assert_eq!(s.sr_highest_slotid, 3);
    assert_ne!(
        s.sr_status_flags & t::SEQ4_STATUS_CB_PATH_DOWN,
        0,
        "no backchannel"
    );
    assert_eq!(
        c.raw(&e, 1, vec![seq(1, 1, false), rootfh()]).status,
        nfsstat4::NFS4ERR_RETRY_UNCACHED_REP
    );
    // An unknown session.
    let r = c.raw(
        &e,
        1,
        vec![nfs_argop4::OP_SEQUENCE(t::SEQUENCE4args {
            sa_sessionid: [7; 16],
            sa_sequenceid: 1,
            sa_slotid: 0,
            sa_highest_slotid: 0,
            sa_cachethis: false,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_BADSESSION);
    // Too many operations for the session.
    let mut many = vec![seq(2, 0, false)];
    many.extend((0..20).map(|_| rootfh()));
    assert_eq!(c.raw(&e, 1, many).status, nfsstat4::NFS4ERR_TOO_MANY_OPS);
}

#[test]
fn opens_with_the_current_stateid() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    c.session(&e, "cur", 0);
    let cur = t::stateid4 {
        seqid: 1,
        other: [0; 12],
    };
    let r = c.run(
        &e,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", "f", true, 3, 0),
            write(&cur, 0, b"abc"),
            read(&cur, 0, 10),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let o = opened(&r);
    assert_eq!(
        o.rflags & t::OPEN4_RESULT_CONFIRM,
        0,
        "4.1 does not confirm"
    );
    let fh = fh_of(&r);
    // The current stateid is gone after a new filehandle.
    let r = c.run(&e, vec![putfh(&fh), read(&cur, 0, 1)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_STATEID);
    // CLAIM_FH opens the current file; seqid 0 matches any.
    let r = c.run(
        &e,
        vec![
            putfh(&fh),
            nfs_argop4::OP_OPEN(t::OPEN4args {
                seqid: 0,
                share_access: 1,
                share_deny: 0,
                owner: c.owner("o2"),
                openhow: t::openflag4::default(t::opentype4::OPEN4_NOCREATE),
                claim: t::open_claim4::CLAIM_FH,
            }),
        ],
    );
    let o2 = opened(&r);
    let mut any = o2.stateid.clone();
    any.seqid = 0;
    let r = c.run(&e, vec![putfh(&fh), read(&any, 0, 3)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    // CLOSE answers the invalid stateid.
    let r = c.run(&e, vec![putfh(&fh), srv4_common::close(0, &o2.stateid)]);
    let nfs_resop4::OP_CLOSE(t::CLOSE4res::NFS4_OK(s)) = &r.resarray[1] else {
        panic!("{r:?}")
    };
    assert_eq!(s.seqid, u32::MAX);
    // TEST_STATEID of one open, one closed, one special.
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_TEST_STATEID(t::TEST_STATEID4args {
            ts_stateids: vec![o.stateid.clone(), o2.stateid.clone(), srv4_common::anon()],
        })],
    );
    let nfs_resop4::OP_TEST_STATEID(t::TEST_STATEID4res::NFS4_OK(ts)) = &r.resarray[0] else {
        panic!("{r:?}")
    };
    assert_eq!(
        ts.tsr_status_codes,
        vec![
            nfsstat4::NFS4_OK,
            nfsstat4::NFS4ERR_BAD_STATEID,
            nfsstat4::NFS4ERR_BAD_STATEID
        ]
    );
    // FREE_STATEID of an open is refused.
    let mut now = o.stateid.clone();
    now.seqid = 0;
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_FREE_STATEID(t::FREE_STATEID4args {
            fsa_stateid: now,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_LOCKS_HELD);
    // Another client's stateid is not this client's.
    let d = V4c::new("10.0.0.2:900", 100, 1);
    d.session(&e, "other", 0);
    let r = d.run(&e, vec![putfh(&fh), read(&any, 0, 1)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_STATEID);
}

#[test]
fn destroying_sessions_and_clients() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    let sid = c.session(&e, "destroy", 0);
    let id = c.clientid.get();
    let r = c.raw(
        &e,
        1,
        vec![nfs_argop4::OP_DESTROY_CLIENTID(t::DESTROY_CLIENTID4args {
            dca_clientid: id,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_CLIENTID_BUSY);
    // Not as the last operation of its own session.
    let r = c.raw(
        &e,
        1,
        vec![
            c.sequence(false),
            nfs_argop4::OP_DESTROY_SESSION(t::DESTROY_SESSION4args { dsa_sessionid: sid }),
            rootfh(),
        ],
    );
    assert_eq!(
        statuses(&r),
        vec![nfsstat4::NFS4_OK, nfsstat4::NFS4ERR_NOT_ONLY_OP]
    );
    // From a connection never bound to it.
    let far = V4c::new("10.0.0.9:900", 100, 1);
    let r = far.raw(
        &e,
        1,
        vec![nfs_argop4::OP_DESTROY_SESSION(t::DESTROY_SESSION4args {
            dsa_sessionid: sid,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_CONN_NOT_BOUND_TO_SESSION);
    let r = c.raw(
        &e,
        1,
        vec![nfs_argop4::OP_DESTROY_SESSION(t::DESTROY_SESSION4args {
            dsa_sessionid: sid,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let r = c.raw(&e, 1, vec![c.sequence(false)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_BADSESSION);
    let r = c.raw(
        &e,
        1,
        vec![nfs_argop4::OP_DESTROY_CLIENTID(t::DESTROY_CLIENTID4args {
            dca_clientid: id,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let r = c.raw(&e, 1, vec![create_session(id, 2, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
}

#[test]
fn reclaim_complete_and_binding() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    let sid = c.session(&e, "rc", 0);
    let rc =
        |one: bool| nfs_argop4::OP_RECLAIM_COMPLETE(t::RECLAIM_COMPLETE4args { rca_one_fs: one });
    assert_eq!(
        c.run(&e, vec![rc(true)]).status,
        nfsstat4::NFS4ERR_NOFILEHANDLE
    );
    assert_eq!(c.run(&e, vec![rc(false)]).status, nfsstat4::NFS4_OK);
    assert_eq!(
        c.run(&e, vec![rc(false)]).status,
        nfsstat4::NFS4ERR_COMPLETE_ALREADY
    );
    // BIND_CONN_TO_SESSION: the fore channel here; a backchannel needs a
    // connection that can carry one.
    let other = V4c::new("10.0.0.1:950", 100, 1);
    let bind = |dir| {
        nfs_argop4::OP_BIND_CONN_TO_SESSION(t::BIND_CONN_TO_SESSION4args {
            bctsa_sessid: sid,
            bctsa_dir: dir,
            bctsa_use_conn_in_rdma_mode: true,
        })
    };
    let r = other.raw(
        &e,
        1,
        vec![bind(t::channel_dir_from_client4::CDFC4_FORE_OR_BOTH)],
    );
    let nfs_resop4::OP_BIND_CONN_TO_SESSION(t::BIND_CONN_TO_SESSION4res::NFS4_OK(b)) =
        &r.resarray[0]
    else {
        panic!("{r:?}")
    };
    assert_eq!(b.bctsr_dir, t::channel_dir_from_server4::CDFS4_FORE);
    assert!(!b.bctsr_use_conn_in_rdma_mode);
    let r = other.raw(&e, 1, vec![bind(t::channel_dir_from_client4::CDFC4_BACK)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_INVAL);
    // BIND_CONN_TO_SESSION is never after SEQUENCE.
    let r = c.raw(
        &e,
        1,
        vec![
            c.sequence(false),
            bind(t::channel_dir_from_client4::CDFC4_FORE),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOT_ONLY_OP);
}

#[test]
fn the_administrator_revokes_state() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    c.session(&e, "admin", 0);
    let r = c.run(
        &e,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", "a", true, 3, 0),
            getfh(),
        ],
    );
    let o = opened(&r);
    let fh = fh_of(&r);
    assert_eq!(e.srv.revoke(e.prog.vfs(), |o| o == b"admin"), 1);
    let r = c.raw(
        &e,
        1,
        vec![c.sequence(false), putfh(&fh), read(&o.stateid, 0, 1)],
    );
    let nfs_resop4::OP_SEQUENCE(t::SEQUENCE4res::NFS4_OK(s)) = &r.resarray[0] else {
        panic!("{r:?}")
    };
    assert_ne!(s.sr_status_flags & t::SEQ4_STATUS_ADMIN_STATE_REVOKED, 0);
    assert_eq!(r.status, nfsstat4::NFS4ERR_ADMIN_REVOKED);
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_FREE_STATEID(t::FREE_STATEID4args {
            fsa_stateid: o.stateid.clone(),
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let r = c.raw(&e, 1, vec![c.sequence(false)]);
    let nfs_resop4::OP_SEQUENCE(t::SEQUENCE4res::NFS4_OK(s)) = &r.resarray[0] else {
        panic!("{r:?}")
    };
    assert_eq!(s.sr_status_flags & t::SEQ4_STATUS_ADMIN_STATE_REVOKED, 0);
    // The file is no longer open: a local job may deny everyone.
    let j = e.w.job(5);
    let fd = j
        .open(
            "/home/x/a",
            ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDWR | ifs::ops::flags::O_SHARE_NONE),
        )
        .unwrap();
    j.close(fd).unwrap();
}
