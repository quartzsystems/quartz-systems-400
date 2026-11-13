// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 3 locks between two client mounts through the server's lock
//! manager; the client's own status monitor watching the server; the
//! server restarting: its monitor's notice reaches the client's lock
//! manager (`NLM_SM_NOTIFY` with the client's tag), which tells the
//! client mounts, and the locks held are reclaimed in the grace period
//! while another client's new lock waits it out and is refused.

#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod cli_common;

use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cli_common::{ServerSys, World, bob};
use ifs::Errno;
use ifs::remote::{
    CreateMode, LockKind, LockOwner, MountCcsid, OpenToken, RemoteError, RemoteFs, SetAttr,
};
use nfs::client::nlm::PRIV_TAG;
use nfs::client::{self, ClientEnv, Mount};
use nfs::exports::mount::parse;
use nfs::nlm::state::{Lock, LockClient, NlmConfig, NlmState, Reply};
use nfs::nsm::record::MemStatdStore;
use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::rpc::{ClientConfig, Credential, Proto, RpcClient};
use nfs::server::locate::{FixedLocator, Host, Locator};
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common, Listeners};
use nfs::server::{ExportTable, SystemClock};
use nfs::types::nsm::SM_PROG::SM_VERS;
use nfs::types::nsm::stat_chge;

/// The client system's lock manager's client side: tells the NFS
/// client of a server's restart, as `QNFSNLMD` does through the service.
#[derive(Default)]
struct Relay {
    told: Mutex<Vec<(Vec<u8>, i32)>>,
}

impl LockClient for Relay {
    fn granted(&self, _peer: SocketAddr, _exclusive: bool, _lock: &Lock) -> bool {
        false
    }
    fn result(&self, _peer: SocketAddr, _procedure: u32, _res: &Reply) {}
    fn restarted(&self, mon_name: &[u8], state: i32, private: [u8; 16]) {
        if private == PRIV_TAG {
            self.told.lock().unwrap().push((mon_name.to_vec(), state));
            client::restart::server_restarted(mon_name, state, false);
        }
    }
}

fn rpc() -> ClientConfig {
    ClientConfig {
        timeout: Duration::from_millis(500),
        retransmits: 2,
        reserved_port: false,
        ..ClientConfig::default()
    }
}

fn mount(s: &ServerSys, host: &str, local: &Arc<FixedLocator>) -> Mount {
    let env = ClientEnv {
        local: Some(Arc::clone(local) as Arc<dyn Locator>),
        ..s.env(host)
    };
    let o = parse("soft,timeo=5,retrans=3").unwrap();
    client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        &env,
    )
    .unwrap()
}

#[test]
#[allow(clippy::too_many_lines)]
fn locks_between_mounts_and_reclaimed_after_a_restart() {
    let s = ServerSys::start_with(true, true, Duration::from_secs(2));
    // The client system's status monitor and lock manager.
    let cw = World::new();
    let local = Arc::new(FixedLocator::new());
    let (sst, _) = NsmState::start(
        Box::new(MemStatdStore::default()),
        NsmConfig {
            me: "client".into(),
            locator: Arc::clone(&local) as Arc<dyn Locator>,
            rpc: rpc(),
            notify_rounds: 1,
            notify_pause: Duration::ZERO,
        },
    )
    .unwrap();
    let nst = NlmState::new(NlmConfig {
        monitor: false,
        locator: Arc::clone(&local) as Arc<dyn Locator>,
        rpc: rpc(),
        grace: Duration::ZERO,
        grant_tries: 1,
        clock: Arc::new(SystemClock),
    });
    let relay = Arc::new(Relay::default());
    nst.set_client(Arc::clone(&relay) as Arc<dyn LockClient>);
    let nlm_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mon_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let lo = Host::Addr(IpAddr::from([127, 0, 0, 1]));
    local.put(lo.clone(), 100_021, 2, nlm_udp.local_addr().unwrap());
    local.put(lo, 100_024, 1, mon_udp.local_addr().unwrap());
    let mon_addr = mon_udp.local_addr().unwrap();
    let stop = AtomicBool::new(false);
    let ready = AtomicUsize::new(0);
    let on_ready = || {
        ready.fetch_add(1, Ordering::SeqCst);
    };
    let exports: Arc<dyn ExportTable> = Arc::clone(&cw.exports) as Arc<dyn ExportTable>;
    std::thread::scope(|sc| {
        sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: Some(TcpListener::bind("127.0.0.1:0").unwrap()),
                    udp: Some(nlm_udp),
                },
                binder: None,
                stop: &stop,
                ready: Some(&on_ready),
            };
            run::nlmd(
                c,
                &exports,
                &nst,
                || cw.vfs(),
                || -> Box<dyn NewsSource> { Box::new(ClientNews(cw.job(1))) },
            )
            .unwrap();
        });
        sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: None,
                    udp: Some(mon_udp),
                },
                binder: None,
                stop: &stop,
                ready: Some(&on_ready),
            };
            run::nsmd(c, &sst, Vec::new()).unwrap();
        });
        let end = Instant::now() + Duration::from_secs(10);
        while ready.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        let check = || -> Result<(), String> {
            let m1 = mount(&s, "client1", &local);
            let m2 = mount(&s, "client2", &local);
            assert!(m1.info().locks);
            let (root, _) = m1.root().map_err(|e| format!("{e:?}"))?;
            let (f, _) = m1
                .create(
                    &bob(),
                    &root,
                    b"locked",
                    CreateMode::Unchecked(SetAttr::default()),
                )
                .unwrap();
            let t = OpenToken::default();
            let o = LockOwner(1);
            m1.lock(&bob(), &f, o, LockKind::Write, 0, 10, &t).unwrap();
            assert_eq!(
                m2.lock(&bob(), &f, o, LockKind::Write, 5, 10, &t)
                    .unwrap_err(),
                RemoteError::Errno(Errno::EAGAIN)
            );
            let c = m2
                .test(&bob(), &f, o, LockKind::Read, 0, 1)
                .unwrap()
                .unwrap();
            assert_eq!((c.kind, c.off, c.len), (LockKind::Write, 0, 10));
            // Another range is free; the same owner of the same mount
            // takes more of its own.
            m2.lock(&bob(), &f, o, LockKind::Read, 20, 10, &t).unwrap();
            m1.lock(&bob(), &f, o, LockKind::Write, 10, 5, &t).unwrap();
            m1.unlock(&bob(), &f, o, 0, 15).unwrap();
            m2.lock(&bob(), &f, o, LockKind::Write, 5, 5, &t).unwrap();
            m2.unlock(&bob(), &f, o, 0, 0).unwrap();
            assert_eq!(m2.locks_held(), 0);
            m1.lock(&bob(), &f, o, LockKind::Write, 0, 10, &t).unwrap();
            assert_eq!(m1.locks_held(), 1);
            // The client's monitor watches the server, with the tag.
            let mon = sst.monitored();
            assert!(
                mon.iter()
                    .any(|r| r.mon_name == b"127.0.0.1" && r.private == PRIV_TAG),
                "{mon:?}"
            );
            // The server restarts: its locks are gone, a grace period begins.
            s.restart();
            // Its monitor tells ours.
            let mut n = RpcClient::new(mon_addr, Proto::Udp, rpc());
            let r = n
                .call_proc::<SM_VERS::Args, SM_VERS::Res>(
                    &Credential::None,
                    &SM_VERS::Args::SM_NOTIFY(stat_chge {
                        mon_name: b"127.0.0.1".to_vec(),
                        state: 7,
                    }),
                )
                .unwrap();
            assert_eq!(r, SM_VERS::Res::SM_NOTIFY);
            let end = Instant::now() + Duration::from_secs(5);
            while relay.told.lock().unwrap().is_empty() {
                assert!(Instant::now() < end, "the restart was not relayed");
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(relay.told.lock().unwrap()[0], (b"127.0.0.1".to_vec(), 7));
            // client2's new lock waits out the grace period and is
            // refused: client1 took its own back meanwhile.
            let t0 = Instant::now();
            assert_eq!(
                m2.lock(&bob(), &f, o, LockKind::Write, 0, 1, &t)
                    .unwrap_err(),
                RemoteError::Errno(Errno::EAGAIN)
            );
            assert!(
                t0.elapsed() >= Duration::from_millis(900),
                "{:?}",
                t0.elapsed()
            );
            assert_eq!(m1.locks_held(), 1);
            m1.unlock(&bob(), &f, o, 0, 10).unwrap();
            m2.lock(&bob(), &f, o, LockKind::Write, 0, 1, &t).unwrap();
            m1.unmount().unwrap();
            m2.unmount().unwrap();
            Ok(())
        };
        let r = check();
        stop.store(true, Ordering::SeqCst);
        r.unwrap();
    });
}

#[test]
fn without_the_local_monitor_locks_are_enolck() {
    let s = ServerSys::start(true, true);
    // The monitor this system would run is not there.
    let local = Arc::new(FixedLocator::new());
    let m = mount(&s, "client1", &local);
    let (root, _) = m.root().unwrap();
    let (f, _) = m
        .create(
            &bob(),
            &root,
            b"f",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    assert_eq!(
        m.lock(
            &bob(),
            &f,
            LockOwner(1),
            LockKind::Write,
            0,
            1,
            &OpenToken::default()
        )
        .unwrap_err(),
        RemoteError::Errno(Errno::ENOLCK)
    );
}
