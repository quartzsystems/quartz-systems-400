// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The version 3 client against the server over loopback, TCP and UDP:
//! every call of the remote file system and its errors, mounting
//! through the binder and without it (`port=`, `mountport=`), the
//! mount's refusals and `retry=`.

#![allow(clippy::too_many_lines)]

mod cli_common;

use std::time::{Duration, Instant};

use cli_common::{ServerSys, bob};
use ifs::Errno;
use ifs::client::IfsClient;
use ifs::remote::{
    CreateMode, LockKind, LockOwner, MountCcsid, OpenToken, RKind, RemoteError, RemoteFs, SetAttr,
    SetTime, Share, Stable, access,
};
use nfs::client::{self, Version};
use nfs::exports::mount::parse;

fn errno(e: Errno) -> RemoteError {
    RemoteError::Errno(e)
}

#[test]
fn every_call_over_tcp_and_udp() {
    let s = ServerSys::start(true, false);
    for proto in ["tcp", "udp"] {
        let o = parse(&format!("vers=3,proto={proto},soft,timeo=10,retrans=2")).unwrap();
        let m = client::connect(
            "127.0.0.1:/home/exp",
            &o,
            MountCcsid::default(),
            &bob(),
            &s.env("cli1"),
        )
        .unwrap();
        assert_eq!(m.version(), Version::V3);
        let i = m.info();
        assert_eq!(i.name, "nfs3");
        assert!(
            i.options.contains(&format!("proto={proto}")),
            "{}",
            i.options
        );
        assert!(i.options.contains("vers=3") && i.options.contains("soft"));
        assert!(!i.stateful_open && !i.keeps_open_unlinked && !i.locks);
        // The root file system names compare without case.
        assert!(!i.case_sensitive);
        assert_eq!(i.slow_after, Duration::from_millis(7000));
        let c = bob();
        let (root, ra) = m.root().unwrap();
        assert_eq!(ra.kind, RKind::Dir);
        let dname = format!("d-{proto}");
        let (d, da) = m
            .mkdir(
                &c,
                &root,
                dname.as_bytes(),
                &SetAttr {
                    mode: Some(0o755),
                    ..SetAttr::default()
                },
            )
            .unwrap();
        // BOB has no group profile: the group bits are 0 there.
        assert_eq!((da.kind, da.mode & 0o777, da.uid), (RKind::Dir, 0o705, 100));
        assert_eq!(
            m.mkdir(&c, &root, dname.as_bytes(), &SetAttr::default())
                .unwrap_err(),
            errno(Errno::EEXIST)
        );
        let mode = SetAttr {
            mode: Some(0o644),
            ..SetAttr::default()
        };
        let (f, fa) = m.create(&c, &d, b"f", CreateMode::Unchecked(mode)).unwrap();
        assert_eq!((fa.kind, fa.size), (RKind::File, 0));
        assert_eq!(
            m.create(&c, &d, b"f", CreateMode::Guarded(mode))
                .unwrap_err(),
            errno(Errno::EEXIST)
        );
        // Exclusive: the same verifier finds its file, another is EEXIST.
        let (x, _) = m.create(&c, &d, b"x", CreateMode::Exclusive(77)).unwrap();
        assert_eq!(
            m.create(&c, &d, b"x", CreateMode::Exclusive(77)).unwrap().0,
            x
        );
        assert_eq!(
            m.create(&c, &d, b"x", CreateMode::Exclusive(78))
                .unwrap_err(),
            errno(Errno::EEXIST)
        );
        // Data, unstable then committed under one verifier.
        let data: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let w = m.write(&c, &f, 0, &data, Stable::Unstable).unwrap();
        assert_eq!(w.count as usize, data.len());
        assert_eq!(m.commit(&c, &f, 0, 0).unwrap(), w.verifier);
        let w2 = m.write(&c, &f, 20_000, b"tail", Stable::FileSync).unwrap();
        assert_eq!(w2.committed, Stable::FileSync);
        let mut got = Vec::new();
        let mut off = 0;
        loop {
            let (b, eof) = m.read(&c, &f, off, 4096).unwrap();
            off += b.len() as u64;
            got.extend(b);
            if eof {
                break;
            }
        }
        assert_eq!(&got[..20_000], &data[..]);
        assert_eq!(&got[20_000..], b"tail");
        assert_eq!(m.getattr(&c, &f).unwrap().size, 20_004);
        // Attributes, with a guard that holds and one that does not.
        let a = m.getattr(&c, &f).unwrap();
        let a2 = m
            .setattr(
                &c,
                &f,
                &SetAttr {
                    size: Some(10),
                    mtime: Some(SetTime::At(1_000_000_000_000_000_000)),
                    ..SetAttr::default()
                },
                Some(a.ctime),
            )
            .unwrap();
        assert_eq!(a2.size, 10);
        assert_eq!(a2.mtime, 1_000_000_000_000_000_000);
        assert_ne!(a2.change, a.change);
        assert_eq!(
            m.setattr(
                &c,
                &f,
                &SetAttr {
                    mode: Some(0o600),
                    ..SetAttr::default()
                },
                Some(a.ctime)
            )
            .unwrap_err(),
            errno(Errno::EAGAIN)
        );
        // Access: the owner reads and writes.
        let bits = m
            .access(&c, &f, access::READ | access::MODIFY | access::EXECUTE)
            .unwrap();
        assert_eq!(bits, access::READ | access::MODIFY);
        // Links.
        let (l, la) = m.symlink(&c, &d, b"ln", b"f", &SetAttr::default()).unwrap();
        assert_eq!(la.kind, RKind::Symlink);
        assert_eq!(m.readlink(&c, &l).unwrap(), b"f");
        let (_, pa) = m.mknod(&c, &d, b"pipe", RKind::Fifo, &mode).unwrap();
        assert_eq!(pa.kind, RKind::Fifo);
        assert_eq!(
            m.mknod(&c, &d, b"sock", RKind::Socket, &mode).unwrap_err(),
            errno(Errno::ENOTSUP)
        );
        m.link(&c, &f, &d, b"hard").unwrap();
        assert_eq!(m.getattr(&c, &f).unwrap().nlink, 2);
        m.rename(&c, &d, b"hard", &root, b"moved").unwrap();
        assert_eq!(m.lookup(&c, &root, b"moved").unwrap().0, f);
        assert_eq!(m.lookup(&c, &d, b"hard").unwrap_err(), errno(Errno::ENOENT));
        // A directory of many names, read in pages.
        for i in 0..300 {
            m.create(
                &c,
                &d,
                format!("n{i:03}").as_bytes(),
                CreateMode::Unchecked(mode),
            )
            .unwrap();
        }
        let mut names = Vec::new();
        let (mut cookie, mut verf) = (0, 0);
        loop {
            let p = m.readdir(&c, &d, cookie, verf, 1024).unwrap();
            for e in &p.entries {
                assert!(e.name != b"." && e.name != b"..");
                names.push(String::from_utf8(e.name.clone()).unwrap());
                cookie = e.cookie;
            }
            verf = p.verifier;
            if p.eof {
                break;
            }
            assert!(!p.entries.is_empty());
        }
        assert_eq!(names.len(), 304, "300 names, f, x, ln, pipe");
        let st = m.statfs(&c, &root).unwrap();
        assert!(st.total_bytes >= st.free_bytes);
        let pc = m.pathconf(&c, &root).unwrap();
        assert_eq!(pc.name_max, 255);
        // Opens mean nothing to version 3; locks need the monitors.
        let t = m.open(&c, &f, Share::default(), LockOwner(1)).unwrap();
        assert_eq!(t, OpenToken::default());
        m.close(&c, &f, &t).unwrap();
        assert_eq!(
            m.lock(&c, &f, LockOwner(1), LockKind::Write, 0, 1, &t)
                .unwrap_err(),
            errno(Errno::ENOLCK)
        );
        // Removals; a removed file's handle is stale.
        assert_eq!(
            m.rmdir(&c, &root, dname.as_bytes()).unwrap_err(),
            errno(Errno::ENOTEMPTY)
        );
        m.remove(&c, &root, b"moved").unwrap();
        m.remove(&c, &d, b"f").unwrap();
        assert_eq!(m.getattr(&c, &f).unwrap_err(), RemoteError::Stale);
        assert_eq!(m.remove(&c, &d, b"f").unwrap_err(), errno(Errno::ENOENT));
        m.unmount().unwrap();
        // Ended: calls are refused.
        assert_eq!(m.getattr(&c, &root).unwrap_err(), RemoteError::Interrupted);
    }
}

#[test]
fn authority_and_squashing_are_the_servers() {
    let s = ServerSys::start(true, false);
    s.world().job(9).mkdir("/home/exp/priv", 0o700).unwrap();
    let o = parse("soft,timeo=10,retrans=2").unwrap();
    let m = client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        &s.env("cli1"),
    )
    .unwrap();
    let (root, _) = m.root().unwrap();
    let (p, _) = m.lookup(&bob(), &root, b"priv").unwrap();
    assert_eq!(
        m.create(&bob(), &p, b"no", CreateMode::Unchecked(SetAttr::default()))
            .unwrap_err(),
        errno(Errno::EACCES)
    );
    // UID 0 is the anonymous user there.
    let (f, a) = m
        .create(
            &cli_common::root(),
            &root,
            b"by-root",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    assert_eq!(a.uid, cli_common::srv3_common::ANON_UID);
    m.remove(&cli_common::root(), &root, b"by-root").unwrap();
    let _ = f;
}

#[test]
fn mounts_refused() {
    let s = ServerSys::start(true, false);
    s.world().write_file("/home/exp/file", b"x");
    let o = parse("soft,timeo=5,retrans=1,retry=1").unwrap();
    let mnt = |mfs: &str| {
        client::connect(mfs, &o, MountCcsid::default(), &bob(), &s.env("cli1")).map(|_| ())
    };
    assert_eq!(
        mnt("127.0.0.1:/home/nowhere").unwrap_err(),
        errno(Errno::EACCES)
    );
    assert_eq!(
        mnt("127.0.0.1:/home/exp/nothere").unwrap_err(),
        errno(Errno::ENOENT)
    );
    assert_eq!(
        mnt("127.0.0.1:/home/exp/file").unwrap_err(),
        errno(Errno::ENOTDIR)
    );
    assert_eq!(mnt("127.0.0.1").unwrap_err(), errno(Errno::EINVAL));
    assert_eq!(
        mnt("no-such-host.invalid:/x").unwrap_err(),
        errno(Errno::EHOSTUNREACH)
    );
    // A directory under the export mounts.
    s.world().job(9).mkdir("/home/exp/sub", 0o777).unwrap();
    mnt("127.0.0.1:/home/exp/sub").unwrap();
    // Version 4 is not offered by this server.
    let o4 = parse("vers=4.0,soft,timeo=5,retrans=1,retry=1").unwrap();
    assert!(
        client::connect(
            "127.0.0.1:/home/exp",
            &o4,
            MountCcsid::default(),
            &bob(),
            &s.env("c")
        )
        .is_err()
    );
}

#[test]
fn ports_given_need_no_binder() {
    let s = ServerSys::start(false, false);
    let o = parse(&format!("soft,timeo=5,retrans=1,{}", s.ports_opts())).unwrap();
    let m = client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        &s.env("cli1"),
    )
    .unwrap();
    assert!(m.info().options.contains(&format!("port={}", s.ports.nfs)));
    let (root, _) = m.root().unwrap();
    m.mkdir(&bob(), &root, b"made", &SetAttr::default())
        .unwrap();
    // Without them the binder is asked, and there is none.
    let o = parse("soft,timeo=2,retrans=1,retry=2").unwrap();
    let t = Instant::now();
    let e = client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        &s.env("cli1"),
    )
    .unwrap_err();
    assert_eq!(e, RemoteError::TimedOut);
    // Two attempts of 0.2 + 0.4 s, and 0.2 s between them.
    let took = t.elapsed();
    assert!(
        took >= Duration::from_millis(1300) && took < Duration::from_secs(5),
        "{took:?}"
    );
}

#[test]
fn retry_waits_for_the_server() {
    let s = ServerSys::start(false, false);
    s.stop();
    let o = parse(&format!(
        "soft,timeo=2,retrans=1,retry=50,{}",
        s.ports_opts()
    ))
    .unwrap();
    let env = s.env("cli1");
    let h = std::thread::spawn(move || {
        client::connect(
            "127.0.0.1:/home/exp",
            &o,
            MountCcsid::default(),
            &bob(),
            &env,
        )
        .map(|m| m.version())
    });
    std::thread::sleep(Duration::from_millis(1500));
    s.run();
    assert_eq!(h.join().unwrap().unwrap(), Version::V3);
    // retry=1: one attempt only.
    s.stop();
    let o = parse(&format!(
        "soft,timeo=2,retrans=1,retry=1,{}",
        s.ports_opts()
    ))
    .unwrap();
    let t = Instant::now();
    assert!(
        client::connect(
            "127.0.0.1:/home/exp",
            &o,
            MountCcsid::default(),
            &bob(),
            &s.env("c")
        )
        .is_err()
    );
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
}
