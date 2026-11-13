// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A version 3 mount and a version 4 mount of the same export at once
//! (`nfsd4` serving both on one port, `mountd` and `nlmd` beside it):
//! each sees what the other changes, and an NLM lock and a v4 `LOCK`
//! conflict both ways.

#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod cli_common;

use std::net::{IpAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cli_common::{ServerSys, bob};
use ifs::Errno;
use ifs::remote::{
    CreateMode, LockKind, LockOwner, MountCcsid, OpenToken, RemoteError, RemoteFs, SetAttr, Share,
    Stable,
};
use nfs::client::{self, ClientEnv, Mount};
use nfs::exports::mount::parse;
use nfs::nsm::record::MemStatdStore;
use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::registry::Registry;
use nfs::rpc::ClientConfig;
use nfs::server::locate::{FixedLocator, Host, Locator};
use nfs::server::run::{self, Common, Listeners};

fn connect(opts: &str, env: &ClientEnv) -> Mount {
    let o = parse(opts).unwrap();
    client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        env,
    )
    .unwrap()
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_v3_and_a_v4_mount_of_one_export() {
    let s = ServerSys::start_v4(true, true, Duration::from_secs(30));
    // This system's status monitor, for the v3 mount's locks.
    let local = Arc::new(FixedLocator::new());
    let (sst, _) = NsmState::start(
        Box::new(MemStatdStore::default()),
        NsmConfig {
            me: "client".into(),
            locator: Arc::clone(&local) as Arc<dyn Locator>,
            rpc: ClientConfig {
                timeout: Duration::from_millis(500),
                retransmits: 1,
                reserved_port: false,
                ..ClientConfig::default()
            },
            notify_rounds: 1,
            notify_pause: Duration::ZERO,
        },
    )
    .unwrap();
    let mon = UdpSocket::bind("127.0.0.1:0").unwrap();
    local.put(
        Host::Addr(IpAddr::from([127, 0, 0, 1])),
        100_024,
        1,
        mon.local_addr().unwrap(),
    );
    let stop = AtomicBool::new(false);
    let ready = AtomicUsize::new(0);
    let on_ready = || {
        ready.fetch_add(1, Ordering::SeqCst);
    };
    std::thread::scope(|sc| {
        sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: None,
                    udp: Some(mon),
                },
                binder: None,
                stop: &stop,
                ready: Some(&on_ready),
            };
            run::nsmd(c, &sst, Vec::new()).unwrap();
        });
        let end = Instant::now() + Duration::from_secs(10);
        while ready.load(Ordering::SeqCst) < 1 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        let body = || {
            let m3 = connect(
                "vers=3,soft,timeo=5,retrans=3",
                &ClientEnv {
                    local: Some(Arc::clone(&local) as Arc<dyn Locator>),
                    ..s.env("c3")
                },
            );
            let reg = Arc::new(Registry::new(
                Arc::new(cli_common::ids()),
                nfs::registry::TTL,
                Arc::new(nfs::server::SystemClock),
            ));
            let m4 = connect(
                &format!("vers=4.1,soft,timeo=5,retrans=3,port={}", s.ports.nfs),
                &ClientEnv {
                    registry: Some(reg),
                    ..s.env("c4")
                },
            );
            let c = bob();
            let (r3, _) = m3.root().unwrap();
            let (r4, _) = m4.root().unwrap();
            // A file made through v3 is read through v4, and back.
            let (f3, _) = m3
                .create(&c, &r3, b"both", CreateMode::Unchecked(SetAttr::default()))
                .unwrap();
            m3.write(&c, &f3, 0, b"from v3", Stable::FileSync).unwrap();
            let (f4, a4) = m4.lookup(&c, &r4, b"both").unwrap();
            assert_eq!((a4.size, a4.uid), (7, 100));
            let rw = Share {
                read: true,
                write: true,
                ..Share::default()
            };
            let t4 = m4.open(&c, &f4, rw, LockOwner(1)).unwrap();
            assert_eq!(m4.read(&c, &f4, 0, 100).unwrap().0, b"from v3");
            m4.write(&c, &f4, 0, b"FROM v4", Stable::FileSync).unwrap();
            assert_eq!(m3.read(&c, &f3, 0, 100).unwrap().0, b"FROM v4");
            m4.mkdir(&c, &r4, b"d4", &SetAttr::default()).unwrap();
            assert!(m3.lookup(&c, &r3, b"d4").is_ok());
            // An NLM lock stops a v4 LOCK, and the other way round.
            let none = OpenToken::default();
            m3.lock(&c, &f3, LockOwner(1), LockKind::Write, 0, 10, &none)
                .unwrap();
            assert_eq!(
                m4.lock(&c, &f4, LockOwner(1), LockKind::Write, 5, 1, &t4)
                    .unwrap_err(),
                RemoteError::Errno(Errno::EAGAIN)
            );
            assert!(
                m4.test(&c, &f4, LockOwner(1), LockKind::Read, 0, 1)
                    .unwrap()
                    .is_some()
            );
            m3.unlock(&c, &f3, LockOwner(1), 0, 10).unwrap();
            m4.lock(&c, &f4, LockOwner(1), LockKind::Write, 5, 1, &t4)
                .unwrap();
            assert_eq!(
                m3.lock(&c, &f3, LockOwner(1), LockKind::Read, 0, 10, &none)
                    .unwrap_err(),
                RemoteError::Errno(Errno::EAGAIN)
            );
            assert!(
                m3.test(&c, &f3, LockOwner(1), LockKind::Read, 5, 1)
                    .unwrap()
                    .is_some()
            );
            m4.unlock(&c, &f4, LockOwner(1), 5, 1).unwrap();
            m3.lock(&c, &f3, LockOwner(1), LockKind::Read, 0, 10, &none)
                .unwrap();
            m3.unlock(&c, &f3, LockOwner(1), 0, 10).unwrap();
            m4.close(&c, &f4, &t4).unwrap();
            m3.unmount().unwrap();
            m4.unmount().unwrap();
        };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        stop.store(true, Ordering::SeqCst);
        if let Err(p) = r {
            std::panic::resume_unwind(p);
        }
    });
}
