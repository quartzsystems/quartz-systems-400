// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The version 4 client against a scripted COMPOUND responder: 4.1
//! (`EXCHANGE_ID`, a session, slots) and 4.0 (`SETCLIENTID`,
//! `OPEN_CONFIRM`), `vers=4` falling back to 4.0, the export found
//! through the pseudo root, the calls, opens and locks, the lease kept,
//! `DELAY` waited out, a lost session made again, and a server restart
//! with opens and locks reclaimed. The scenarios take any v4 server's
//! address: the full-stack runs against NFS-6's server are ignored
//! until it exists.

#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod cli4_common;
mod cli_common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cli_common::ServerSys;
use cli4_common::Fake4;
use ifs::Errno;
use ifs::remote::{
    CreateMode, Cred, LockKind, LockOwner, MountCcsid, RKind, RemoteError, RemoteFs, SetAttr,
    Share, Stable,
};
use nfs::client::{self, ClientEnv, Mount, Version};
use nfs::exports::mount::parse;
use nfs::registry::Registry;
use nfs::types::nfs4::nfsstat4;

fn bob() -> Cred {
    Cred {
        uid: 100,
        gid: 100,
        gids: vec![100],
    }
}

fn env(host: &str) -> ClientEnv {
    ClientEnv {
        host_name: host.into(),
        reserved_port: false,
        ..ClientEnv::default()
    }
}

fn mount(
    addr: SocketAddr,
    vers: &str,
    host: &str,
    reg: Option<&Arc<Registry>>,
) -> Result<Mount, RemoteError> {
    let o = parse(&format!(
        "vers={vers},soft,timeo=10,retrans=3,retry=1,port={}",
        addr.port()
    ))
    .unwrap();
    client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        &ClientEnv {
            registry: reg.cloned(),
            ..env(host)
        },
    )
}

/// Files, names and data through a v4 mount of `addr`.
fn files(addr: SocketAddr, vers: &str, reg: Option<&Arc<Registry>>) {
    let m = mount(addr, vers, "c1", reg).unwrap();
    let i = m.info();
    assert_eq!(i.name, "nfs4");
    assert!(i.stateful_open && i.keeps_open_unlinked && i.locks);
    assert!(i.options.contains(&format!("vers={}", m.version().text())));
    let c = bob();
    let (root, a) = m.root().unwrap();
    assert_eq!(a.kind, RKind::Dir);
    let (d, da) = m.mkdir(&c, &root, b"d", &SetAttr::default()).unwrap();
    assert_eq!(da.kind, RKind::Dir);
    let (f, fa) = m
        .create(&c, &d, b"f", CreateMode::Guarded(SetAttr::default()))
        .unwrap();
    assert_eq!((fa.kind, fa.uid), (RKind::File, 100));
    assert_eq!(
        m.create(&c, &d, b"f", CreateMode::Guarded(SetAttr::default()))
            .unwrap_err(),
        RemoteError::Errno(Errno::EEXIST)
    );
    assert_eq!(m.lookup(&c, &d, b"f").unwrap().0, f);
    let share = Share {
        read: true,
        write: true,
        ..Share::default()
    };
    let t = m.open(&c, &f, share, LockOwner(1)).unwrap();
    assert!(!t.0.is_empty());
    let w = m.write(&c, &f, 0, b"hello, v4", Stable::Unstable).unwrap();
    assert_eq!(w.count, 9);
    assert_eq!(m.commit(&c, &f, 0, 0).unwrap(), w.verifier);
    assert_eq!(
        m.read(&c, &f, 0, 100).unwrap(),
        (b"hello, v4".to_vec(), true)
    );
    let a = m
        .setattr(
            &c,
            &f,
            &SetAttr {
                size: Some(5),
                ..SetAttr::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(a.size, 5);
    m.close(&c, &f, &t).unwrap();
    // A listing of more than a page.
    for k in 0..120 {
        m.create(
            &c,
            &d,
            format!("n{k:03}").as_bytes(),
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    }
    let (mut cookie, mut verf, mut n) = (0, 0, 0);
    loop {
        let p = m.readdir(&c, &d, cookie, verf, 2048).unwrap();
        for e in &p.entries {
            assert!(e.handle.is_some() && e.attr.is_some());
            cookie = e.cookie;
            n += 1;
        }
        verf = p.verifier;
        if p.eof {
            break;
        }
    }
    assert_eq!(n, 121);
    m.rename(&c, &d, b"f", &root, b"g").unwrap();
    assert_eq!(
        m.lookup(&c, &d, b"f").unwrap_err(),
        RemoteError::Errno(Errno::ENOENT)
    );
    assert_eq!(
        m.rmdir(&c, &root, b"g").unwrap_err(),
        RemoteError::Errno(Errno::ENOTDIR)
    );
    assert_eq!(
        m.remove(&c, &root, b"d").unwrap_err(),
        RemoteError::Errno(Errno::EISDIR)
    );
    m.remove(&c, &root, b"g").unwrap();
    m.unmount().unwrap();
}

/// Locks between two mounts of `addr`.
fn locks(addr: SocketAddr, vers: &str, reg: Option<&Arc<Registry>>) {
    let m1 = mount(addr, vers, "c1", reg).unwrap();
    let m2 = mount(addr, vers, "c2", reg).unwrap();
    let c = bob();
    let (root, _) = m1.root().unwrap();
    let (f, _) = m1
        .create(
            &c,
            &root,
            b"locked",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    let f2 = m2.lookup(&c, &m2.root().unwrap().0, b"locked").unwrap().0;
    let rw = Share {
        read: true,
        write: true,
        ..Share::default()
    };
    let t1 = m1.open(&c, &f, rw, LockOwner(1)).unwrap();
    let t2 = m2.open(&c, &f2, rw, LockOwner(1)).unwrap();
    m1.lock(&c, &f, LockOwner(1), LockKind::Write, 0, 10, &t1)
        .unwrap();
    assert_eq!(
        m2.lock(&c, &f2, LockOwner(1), LockKind::Write, 5, 10, &t2)
            .unwrap_err(),
        RemoteError::Errno(Errno::EAGAIN)
    );
    let x = m2
        .test(&c, &f2, LockOwner(1), LockKind::Write, 0, 1)
        .unwrap()
        .unwrap();
    assert_eq!((x.off, x.len), (0, 10));
    m1.unlock(&c, &f, LockOwner(1), 0, 10).unwrap();
    m2.lock(&c, &f2, LockOwner(1), LockKind::Write, 5, 10, &t2)
        .unwrap();
    assert_eq!(m2.locks_held(), 1);
    m2.unlock(&c, &f2, LockOwner(1), 0, 0).unwrap();
    m1.close(&c, &f, &t1).unwrap();
    m2.close(&c, &f2, &t2).unwrap();
}

#[test]
fn files_over_4_1_and_4_0() {
    let fake = Fake4::new(&[0, 1]);
    let (addr, _stop) = fake.serve();
    files(addr, "4.1", None);
    let fake = Fake4::new(&[0, 1]);
    let (addr, _stop) = fake.serve();
    files(addr, "4.0", None);
    assert!(fake.seen().setclientid >= 1);
}

#[test]
fn locks_over_4_1_and_4_0() {
    for v in ["4.1", "4.0"] {
        let fake = Fake4::new(&[0, 1]);
        let (addr, _stop) = fake.serve();
        locks(addr, v, None);
    }
}

#[test]
fn vers_4_takes_4_1_then_4_0() {
    let both = Fake4::new(&[0, 1]);
    let (a, _s1) = both.serve();
    assert_eq!(mount(a, "4", "c", None).unwrap().version(), Version::V41);
    let old = Fake4::new(&[0]);
    let (b, _s2) = old.serve();
    assert_eq!(mount(b, "4", "c", None).unwrap().version(), Version::V40);
    assert_eq!(
        mount(b, "4.1", "c", None).unwrap_err(),
        RemoteError::Errno(Errno::EPROTONOSUPPORT)
    );
}

#[test]
fn delay_is_waited_out_and_a_lost_session_made_again() {
    let fake = Fake4::new(&[1]);
    let (addr, _stop) = fake.serve();
    let m = mount(addr, "4.1", "c", None).unwrap();
    let (root, _) = m.root().unwrap();
    fake.fail_op(nfsstat4::NFS4ERR_DELAY);
    m.getattr(&bob(), &root).unwrap();
    assert_eq!(fake.seen().delays, 1);
    let before = fake.seen();
    fake.fail_sequence(nfsstat4::NFS4ERR_BADSESSION);
    m.getattr(&bob(), &root).unwrap();
    let after = fake.seen();
    assert_eq!(after.create_session, before.create_session + 1);
    assert_eq!(after.exchange_id, before.exchange_id, "the client ID stays");
    // Many calls at once share the slots.
    let m = std::sync::Arc::new(m);
    let ts: Vec<_> = (0..12)
        .map(|_| {
            let (m, r) = (std::sync::Arc::clone(&m), root.clone());
            std::thread::spawn(move || {
                for _ in 0..20 {
                    m.getattr(&bob(), &r).unwrap();
                }
            })
        })
        .collect();
    for t in ts {
        t.join().unwrap();
    }
    assert_eq!(fake.seen().seq_misordered, 0);
}

#[test]
fn the_lease_is_kept() {
    for (v, minors) in [("4.1", &[1][..]), ("4.0", &[0][..])] {
        let fake = Fake4::new(minors);
        let (addr, _stop) = fake.serve();
        let _m = mount(addr, v, "c", None).unwrap();
        // The responder's lease is 2 s: renewed at 1.33 s.
        std::thread::sleep(Duration::from_millis(1800));
        let s = fake.seen();
        assert!(s.sequence_only + s.renew >= 1, "{v}: {s:?}");
    }
}

/// A restart of the server behind `addr` (by `restart`) with an open and
/// a lock held: both are reclaimed in the grace period.
fn restart_reclaims(fake: &Fake4, addr: SocketAddr, vers: &str) {
    let m = mount(addr, vers, "c1", None).unwrap();
    let c = bob();
    let (root, _) = m.root().unwrap();
    let (f, _) = m
        .create(
            &c,
            &root,
            b"held",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    let rw = Share {
        read: true,
        write: true,
        ..Share::default()
    };
    let t = m.open(&c, &f, rw, LockOwner(4)).unwrap();
    m.lock(&c, &f, LockOwner(4), LockKind::Write, 0, 100, &t)
        .unwrap();
    fake.restart(Duration::from_secs(3));
    // The next call (or the lease's renewal) finds the client gone.
    let end = Instant::now() + Duration::from_secs(10);
    while fake.seen().reclaim_locks == 0 {
        let _ = m.getattr(&c, &f);
        assert!(
            Instant::now() < end,
            "nothing was reclaimed: {:?}",
            fake.seen()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let s = fake.seen();
    assert!(s.reclaim_opens >= 1 && s.reclaim_locks >= 1, "{s:?}");
    assert_eq!(fake.locks(), 1);
    if vers == "4.1" {
        assert!(s.reclaim_complete >= 2, "{s:?}");
    }
    // The open goes on working.
    m.write(&c, &f, 0, b"after", Stable::FileSync).unwrap();
    m.unlock(&c, &f, LockOwner(4), 0, 100).unwrap();
    m.close(&c, &f, &t).unwrap();
}

#[test]
fn a_restart_is_reclaimed_over_4_1_and_4_0() {
    for v in ["4.1", "4.0"] {
        let fake = Fake4::new(&[0, 1]);
        let (addr, _stop) = fake.serve();
        restart_reclaims(&fake, addr, v);
    }
}

/// NFS-6's server (`nfsd4` over `IfsVfs`) on loopback, exporting
/// `/home/exp`; its registry maps `BOB` and the others of its world.
fn v4_server() -> (Arc<ServerSys>, SocketAddr, Arc<Registry>) {
    let s = ServerSys::start_v4(false, false, Duration::from_secs(30));
    let addr = SocketAddr::from(([127, 0, 0, 1], s.ports.nfs));
    let reg = Arc::new(Registry::new(
        Arc::new(cli_common::ids()),
        nfs::registry::TTL,
        Arc::new(nfs::server::SystemClock),
    ));
    (s, addr, reg)
}

#[test]
fn files_against_the_v4_server() {
    for v in ["4.1", "4.0"] {
        let (_s, addr, reg) = v4_server();
        files(addr, v, Some(&reg));
    }
}

#[test]
fn locks_against_the_v4_server() {
    for v in ["4.1", "4.0"] {
        let (_s, addr, reg) = v4_server();
        locks(addr, v, Some(&reg));
    }
}

#[test]
fn owners_are_numbers_without_a_registry() {
    let (_s, addr, _) = v4_server();
    let m = mount(addr, "4.1", "c1", None).unwrap();
    let (root, _) = m.root().unwrap();
    let (_, a) = m
        .create(
            &bob(),
            &root,
            b"num",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    // The server writes BOB@example.com: unknown without a registry.
    assert_eq!(a.uid, Cred::NOBODY);
}
