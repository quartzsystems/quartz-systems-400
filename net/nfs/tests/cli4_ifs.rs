// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The whole version 4 client (4.1 and 4.0) through the IFS against
//! NFS-6's server (`nfsd4` over `IfsVfs`): a client system (an `Ifs`
//! over a `MemBackend` with the NFS client as its factory, owners mapped
//! through a registry) mounts the export of a server system on
//! loopback, and its jobs work on the files:
//! data, names, 2,000 entries, links, two jobs on one file,
//! close-to-open consistency, exclusive create, `ro`, `nosuid`, root
//! squashing, a handle gone stale, a hard mount through a stop (one
//! `QSYSOPR` report each way) and a restart (a new server instance: the client ID or session made
//! again, the open reclaimed in the grace period) whose new write
//! verifier has the client send its data again.

#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod cli_common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cli_common::ServerSys;
use cli_common::srv3_common::ANON_UID;
use ifs::Errno;
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs, read_all, write_all};
use ifs::ops::audit::MemAudit;
use ifs::ops::flags::{O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use ifs::ops::{Config, Ifs, OpenArgs, Owner, RenameMode};
use ifs::remote::{MountCcsid, RemoteConfig, RemoteEvent};
use ifs::vfs::qsys::MemQsys;
use nfs::client::{ClientEnv, Factory};
use nfs::registry::Registry;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// The client system.
struct Client {
    ifs: Arc<Ifs<MemBackend>>,
    env: Arc<MemEnv>,
    events: Arc<Mutex<Vec<RemoteEvent>>>,
}

fn user(name: &str) -> Principal {
    let mut p = Principal {
        user: name.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if name == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0;
    }
    p
}

impl Client {
    fn new(s: &ServerSys) -> Self {
        let env = Arc::new(
            MemEnv::default()
                .user("QSYS", 2)
                .user("QSECOFR", 0)
                .user("BOB", 100)
                .user("QDFTOWN", 3),
        );
        let ifs = Ifs::new(
            MemBackend::new(),
            Box::new(MemQsys::new(Auth::USE)),
            Config::default(),
        );
        ifs.ensure_layout(&*env).unwrap();
        let ifs = Arc::new(ifs);
        let f = Factory::fixed(ClientEnv {
            registry: Some(Arc::new(Registry::new(
                Arc::new(cli_common::ids()),
                nfs::registry::TTL,
                Arc::new(nfs::server::SystemClock),
            ))),
            ..s.env("client")
        });
        ifs.set_remote_factory(
            Some(Arc::new(f)),
            RemoteConfig {
                threads: 4,
                ..RemoteConfig::default()
            },
        );
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev = Arc::clone(&events);
        ifs.set_remote_events(Some(Arc::new(move |e: &RemoteEvent| {
            ev.lock().unwrap().push(e.clone());
        })));
        let c = Self { ifs, env, events };
        for d in ["/mnt", "/mnt2", "/mnt3"] {
            c.job("QSECOFR", 900).mkdir(d, 0o777).unwrap();
        }
        c
    }

    fn job(&self, name: &str, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: Arc::new((*self.env).clone()),
            audit: Arc::new(MemAudit::default()),
            principal: user(name),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }

    fn mount(&self, s: &ServerSys, over: &str, opts: &str) {
        self.job("QSECOFR", 900)
            .mount_remote(
                "*NFS",
                "127.0.0.1:/home/exp",
                over,
                &format!("{opts},port={}", s.ports.nfs),
                MountCcsid::default(),
            )
            .unwrap();
    }
}

fn put(c: &dyn IfsClient, path: &str, data: &[u8]) {
    let fd = c
        .open(
            path,
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o644),
        )
        .unwrap();
    write_all(c, fd, data).unwrap();
    c.close(fd).unwrap();
}

fn get(c: &dyn IfsClient, path: &str) -> Vec<u8> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).unwrap();
    let d = read_all(c, fd).unwrap();
    c.close(fd).unwrap();
    d
}

const QUICK: &str = "soft,timeo=10,retrans=3,acregmin=0,acregmax=0,acdirmin=0,acdirmax=0";

#[allow(clippy::too_many_lines)]
fn files_and_names(vers: &str) {
    let quick = format!("vers={vers},{QUICK}");
    let s = ServerSys::start_v4(false, false, Duration::from_secs(30));
    let c = Client::new(&s);
    c.mount(&s, "/mnt", &quick);
    let m = c
        .job("QSECOFR", 900)
        .mounts()
        .unwrap()
        .into_iter()
        .find(|m| m.at == "/mnt")
        .unwrap();
    assert_eq!(m.kind, "*NFS");
    assert_eq!(m.source, "127.0.0.1:/home/exp");
    assert!(m.options.contains(&format!("vers={vers}")), "{}", m.options);
    assert_eq!(m.responding, Some(true));
    let w = s.world();
    let bob = c.job("BOB", 1);
    put(&bob, "/mnt/a.txt", b"hello");
    assert_eq!(w.read_file("/home/exp/a.txt"), b"hello");
    assert_eq!(get(&bob, "/mnt/a.txt"), b"hello");
    // Append, truncate.
    let fd = bob
        .open("/mnt/a.txt", OpenArgs::new(O_WRONLY | O_APPEND))
        .unwrap();
    write_all(&bob, fd, b", world").unwrap();
    bob.close(fd).unwrap();
    assert_eq!(w.read_file("/home/exp/a.txt"), b"hello, world");
    let fd = bob.open("/mnt/a.txt", OpenArgs::new(O_RDWR)).unwrap();
    bob.ftruncate(fd, 5).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(w.read_file("/home/exp/a.txt"), b"hello");
    assert_eq!(bob.stat("/mnt/a.txt", true).unwrap().uid, 100);
    // Names.
    bob.mkdir("/mnt/d", 0o755).unwrap();
    bob.rename("/mnt/a.txt", "/mnt/d/b.txt", RenameMode::Keep)
        .unwrap();
    assert_eq!(w.read_file("/home/exp/d/b.txt"), b"hello");
    bob.symlink("b.txt", "/mnt/d/ln").unwrap();
    assert_eq!(bob.readlink("/mnt/d/ln").unwrap(), "b.txt");
    assert_eq!(get(&bob, "/mnt/d/ln"), b"hello");
    bob.link("/mnt/d/b.txt", "/mnt/d/hard").unwrap();
    assert_eq!(bob.stat("/mnt/d/hard", true).unwrap().nlink, 2);
    assert_eq!(bob.rmdir("/mnt/d"), Err(Errno::ENOTEMPTY));
    bob.unlink("/mnt/d/hard").unwrap();
    bob.unlink("/mnt/d/ln").unwrap();
    bob.unlink("/mnt/d/b.txt").unwrap();
    bob.rmdir("/mnt/d").unwrap();
    assert!(w.job(9).stat("/home/exp/d", false).is_err());
    // Exclusive create.
    let fd = bob
        .open(
            "/mnt/x",
            OpenArgs::new(O_WRONLY | O_CREAT | O_EXCL).mode(0o600),
        )
        .unwrap();
    bob.close(fd).unwrap();
    assert_eq!(
        bob.open(
            "/mnt/x",
            OpenArgs::new(O_WRONLY | O_CREAT | O_EXCL).mode(0o600)
        ),
        Err(Errno::EEXIST)
    );
    // 2,000 entries made on the server, listed here.
    w.job(9).mkdir("/home/exp/many", 0o777).unwrap();
    for i in 0..2000 {
        w.write_file(&format!("/home/exp/many/f{i:04}"), b"");
    }
    let mut names: Vec<String> = bob
        .read_dir("/mnt/many")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    assert_eq!(names.len(), 2000);
    assert_eq!(names[0], "f0000");
    assert_eq!(names[1999], "f1999");
    // Two jobs on one file.
    let ann = c.job("BOB", 2);
    let fw = bob
        .open("/mnt/shared", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    write_all(&bob, fw, b"first").unwrap();
    bob.fsync(fw, false).unwrap();
    assert_eq!(get(&ann, "/mnt/shared"), b"first");
    bob.close(fw).unwrap();
    // Close-to-open: a change made on the server is seen at the next open.
    w.write_file("/home/exp/shared", b"changed on the server");
    assert_eq!(get(&ann, "/mnt/shared"), b"changed on the server");
    // Root squashing: QSECOFR is UID 0, the anonymous user there.
    put(&c.job("QSECOFR", 3), "/mnt/by-secofr", b"x");
    assert_eq!(
        w.job(9).stat("/home/exp/by-secofr", false).unwrap().uid,
        ANON_UID
    );
    // A handle gone stale: the file is removed on the server.
    put(&bob, "/mnt/gone", b"soon gone");
    let fd = bob.open("/mnt/gone", OpenArgs::new(O_RDONLY)).unwrap();
    w.job(9).unlink("/home/exp/gone").unwrap();
    assert_eq!(bob.read(fd, 100), Err(Errno::ESTALE));
    let _ = bob.close(fd);
    assert_eq!(bob.stat("/mnt/gone", true), Err(Errno::ENOENT));
    // Read-only and nosuid mounts of the same export.
    c.mount(&s, "/mnt2", &format!("ro,{quick}"));
    assert_eq!(get(&bob, "/mnt2/shared"), b"changed on the server");
    assert_eq!(
        bob.open("/mnt2/new", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644)),
        Err(Errno::EROFS)
    );
    c.mount(&s, "/mnt3", &format!("nosuid,{quick}"));
    put(&bob, "/mnt3/prog", b"#!");
    bob.chmod("/mnt3/prog", 0o4755).unwrap();
    assert_eq!(
        w.job(9).stat("/home/exp/prog", false).unwrap().mode & 0o4000,
        0
    );
    // Unmount: every mount of the type.
    let sec = c.job("QSECOFR", 900);
    for at in ["/mnt", "/mnt2", "/mnt3"] {
        assert_eq!(sec.unmount_kind("*NFS", at).unwrap(), 1);
    }
    assert!(sec.mounts().unwrap().iter().all(|m| m.kind != "*NFS"));
}

fn hard_mount(vers: &str) {
    let s = ServerSys::start_v4(false, false, Duration::from_secs(30));
    let c = Client::new(&s);
    // Reported after 0.1 + 0.2 seconds of no answer.
    c.mount(
        &s,
        "/mnt",
        &format!("vers={vers},hard,timeo=1,retrans=1,acregmin=0,acregmax=0,acdirmin=0,acdirmax=0"),
    );
    let bob = c.job("BOB", 1);
    put(&bob, "/mnt/f", b"before");
    s.stop();
    let t = {
        let (ifs, env) = (Arc::clone(&c.ifs), (*c.env).clone());
        std::thread::spawn(move || {
            let bob = LocalIfs {
                ifs,
                env: Arc::new(env),
                audit: Arc::new(MemAudit::default()),
                principal: user("BOB"),
                owner: Owner(2),
                job_ccsid: 37,
            };
            bob.stat("/mnt/f", true).map(|st| st.size)
        })
    };
    std::thread::sleep(Duration::from_millis(1500));
    assert!(!t.is_finished(), "a hard mount waits");
    s.run();
    assert_eq!(t.join().unwrap().unwrap(), 6);
    let evs = c.events.lock().unwrap().clone();
    let downs = evs
        .iter()
        .filter(|e| matches!(e, RemoteEvent::NotResponding { .. }))
        .count();
    let ups = evs
        .iter()
        .filter(|e| matches!(e, RemoteEvent::Responding { .. }))
        .count();
    assert_eq!((downs, ups), (1, 1), "{evs:?}");
    // Data written behind, unstable; the server restarts and loses it.
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
    let fd = bob
        .open(
            "/mnt/big",
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o644),
        )
        .unwrap();
    write_all(&bob, fd, &data).unwrap();
    let w = s.world();
    let mut seen = 0;
    for _ in 0..500 {
        seen = w.job(9).stat("/home/exp/big", false).map_or(0, |s| s.size);
        if seen >= 4 * 32 * 1024 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(seen > 0, "nothing was written behind");
    s.restart();
    // What the server had of it is gone with the restart.
    s.world().write_file("/home/exp/big", b"");
    bob.fsync(fd, false).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(s.world().read_file("/home/exp/big"), data);
}

#[test]
fn files_and_names_over_4_1() {
    files_and_names("4.1");
}

#[test]
fn files_and_names_over_4_0() {
    files_and_names("4.0");
}

#[test]
fn a_hard_4_1_mount_waits_out_a_stop_and_a_restart() {
    hard_mount("4.1");
}

#[test]
fn a_hard_4_0_mount_waits_out_a_stop_and_a_restart() {
    hard_mount("4.0");
}
