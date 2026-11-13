// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's NFS server in this process on loopback: a binder (TCP and UDP),
//! `mountd`, `nfsd`, `nlmd` and `nsmd` from `nfs::server::run`, over an IFS
//! in memory with one export, `/home/exp`. The server is the code under
//! test; nothing of it reaches nfstest's binary.

#![allow(dead_code)]

use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::{Config, Ifs, Owner};
use ifs::vfs::qsys::MemQsys;
use nfs::mount::list::{MemMountList, MountList};
use nfs::nlm::state::{NlmConfig, NlmState};
use nfs::nsm::record::MemStatdStore;
use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::rpc::record::MAX_RECORD;
use nfs::rpc::{ClientConfig, Dispatcher, serve_datagram, serve_tcp};
use nfs::rpcb::Binder;
use nfs::rpcb::uaddr::to_uaddr;
use nfs::server::ifsvfs::{self, IfsVfs};
use nfs::server::locate::{FixedLocator, Host};
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common, Listeners, Registration};
use nfs::server::{Anon, Clock, ExportTable, Gate, Instance, MemExport, MemExports, SystemClock};
use nfs::types::rpcb::rpcb;
use nfs::v3::Nfs3;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

/// `QNFSANON`'s UID.
pub const ANON_UID: u32 = 4_294_770_710;
/// The export.
pub const EXPORT: &str = "/home/exp";

fn principal(name: &str) -> Principal {
    let mut p = Principal {
        user: name.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if name == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0 | SpecialAuth::SECADM.0;
    }
    p
}

fn env() -> MemEnv {
    MemEnv::default()
        .user("QSECOFR", 0)
        .user("QSYS", 101)
        .user("BOB", 100)
        .user("ANN", 103)
        .user("QNFSANON", ANON_UID)
        .user("QDFTOWN", 104)
        .group("DEV", 100, 500)
        .principal(principal("QSECOFR"))
}

/// How the server is set up.
#[derive(Clone, Debug)]
pub struct Options {
    /// NLM's grace period.
    pub grace: Duration,
    /// Where the server calls nfstest's lock manager back (port).
    pub callback_nlm: Option<u16>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            grace: Duration::ZERO,
            callback_nlm: None,
        }
    }
}

/// The running server.
pub struct Server {
    /// The binder's port.
    pub pmap_port: u16,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    binder_stop: Arc<AtomicBool>,
    binder: Vec<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        // The daemons first: they unregister with the binder as they end.
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        self.binder_stop.store(true, Ordering::SeqCst);
        for t in self.binder.drain(..) {
            let _ = t.join();
        }
    }
}

struct World {
    ifs: Arc<Ifs<MemBackend>>,
    env: MemEnv,
    instance: Arc<Instance>,
    exports: Arc<dyn ExportTable>,
}

impl World {
    fn job(&self, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: Arc::new(self.env.clone()),
            audit: Arc::new(MemAudit::default()),
            principal: principal("QSECOFR"),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }

    fn vfs(&self) -> IfsVfs<LocalIfs<MemBackend>> {
        IfsVfs::new(
            self.job(1),
            Arc::clone(&self.instance),
            ifsvfs::Config::default(),
        )
    }

    fn gate(&self) -> Gate {
        Gate::new(Arc::clone(&self.exports))
    }
}

/// A free loopback port (TCP and UDP both free at the time of asking).
pub fn free_port() -> u16 {
    loop {
        let t = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = t.local_addr().unwrap().port();
        if UdpSocket::bind(("127.0.0.1", p)).is_ok() {
            return p;
        }
    }
}

fn listeners() -> Listeners {
    Listeners {
        tcp: Some(TcpListener::bind("127.0.0.1:0").unwrap()),
        udp: Some(UdpSocket::bind("127.0.0.1:0").unwrap()),
    }
}

/// Starts everything and waits until the daemons are registered.
#[allow(clippy::too_many_lines)]
pub fn start(o: &Options) -> Server {
    let backend = MemBackend::new();
    let env = env();
    let ifs = Ifs::new(
        backend,
        Box::new(MemQsys::new(Auth::USE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    let table = Arc::new(MemExports::new());
    let exports: Arc<dyn ExportTable> = Arc::clone(&table) as Arc<dyn ExportTable>;
    let w = Arc::new(World {
        ifs: Arc::new(ifs),
        env,
        instance: Arc::new(Instance::new()),
        exports: Arc::clone(&exports),
    });
    let j = w.job(9);
    j.mkdir(EXPORT, 0o777).unwrap();
    j.chmod(EXPORT, 0o777).unwrap();
    let s = j.stat(EXPORT, false).unwrap();
    let mut e = MemExport::new(
        1,
        EXPORT,
        FileId {
            fs: s.vfs,
            ino: s.ino,
            generation: s.ino_gen,
        },
    );
    // The anonymous user the service names (a LocalIfs names none).
    e.anon = Anon::Uid(ANON_UID);
    table.put(e);
    let mounts: Arc<dyn MountList> = Arc::new(MemMountList::new());
    let stop = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();
    let binder_stop = Arc::new(AtomicBool::new(false));
    let mut binder = Vec::new();

    // The binder, TCP and UDP.
    let bu = UdpSocket::bind("127.0.0.1:0").unwrap();
    let baddr = bu.local_addr().unwrap();
    let bt = TcpListener::bind(baddr).unwrap();
    bu.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    bt.set_nonblocking(true).unwrap();
    // The binder's own entries, as QNFSRPCD makes them (the oracle's
    // /etc/RPCBTAB: versions 2 and 3).
    let b = Binder::default();
    for v in [2, 3] {
        for netid in [&b"tcp"[..], b"udp"] {
            b.register(rpcb {
                r_prog: 100_000,
                r_vers: v,
                r_netid: netid.to_vec(),
                r_addr: to_uaddr(baddr).into_bytes(),
                r_owner: b"0".to_vec(),
            });
        }
    }
    let bd = Arc::new({
        let mut d = Dispatcher::new();
        d.add(Arc::new(b));
        d
    });
    {
        let (stop, bd) = (Arc::clone(&binder_stop), Arc::clone(&bd));
        binder.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let _ = serve_datagram(&bu, &bd);
            }
        }));
    }
    {
        let (stop, bd) = (Arc::clone(&binder_stop), Arc::clone(&bd));
        binder.push(std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match bt.accept() {
                    Ok((s, _)) => {
                        let bd = Arc::clone(&bd);
                        std::thread::spawn(move || {
                            let _ = s.set_nonblocking(false);
                            let _ = serve_tcp(&s, &bd, MAX_RECORD);
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        }));
    }
    let reg = |owner: &str| {
        Some(Registration {
            binder: baddr,
            owner: owner.into(),
        })
    };

    // NSM, then NLM watching hosts through it.
    let nsm_l = listeners();
    let nsm_addr = nsm_l.udp.as_ref().unwrap().local_addr().unwrap();
    let locator = Arc::new(FixedLocator::new());
    let lo = IpAddr::from([127, 0, 0, 1]);
    locator.put(Host::Addr(lo), 100_024, 1, nsm_addr);
    if let Some(p) = o.callback_nlm {
        let cb = SocketAddr::new(lo, p);
        for v in [1, 3, 4] {
            locator.put(Host::Addr(lo), 100_021, v, cb);
            locator.put(Host::Name("127.0.0.1".into()), 100_021, v, cb);
        }
    }
    let rpc = ClientConfig {
        timeout: Duration::from_millis(500),
        retransmits: 1,
        reserved_port: false,
        ..ClientConfig::default()
    };
    let (nsm_state, _) = NsmState::start(
        Box::new(MemStatdStore::default()),
        NsmConfig {
            me: "qs400".into(),
            locator: Arc::clone(&locator) as Arc<dyn nfs::server::locate::Locator>,
            rpc: rpc.clone(),
            notify_rounds: 1,
            notify_pause: Duration::ZERO,
        },
    )
    .unwrap();
    let nlm_state = NlmState::new(NlmConfig {
        monitor: true,
        locator: Arc::clone(&locator) as Arc<dyn nfs::server::locate::Locator>,
        rpc,
        grace: o.grace,
        grant_tries: 3,
        clock: Arc::new(SystemClock) as Arc<dyn Clock>,
    });

    let ready = {
        let started = Arc::clone(&started);
        move || {
            started.fetch_add(1, Ordering::SeqCst);
        }
    };
    {
        let (stop, ready) = (Arc::clone(&stop), ready.clone());
        let w = Arc::clone(&w);
        let reg = reg("QNFSNFSD");
        threads.push(std::thread::spawn(move || {
            let c = Common {
                listeners: listeners(),
                binder: reg,
                stop: &stop,
                ready: Some(&ready),
            };
            run::nfsd(c, || Nfs3::new(w.vfs(), w.gate())).unwrap();
        }));
    }
    {
        let (stop, ready) = (Arc::clone(&stop), ready.clone());
        let (w, exports, mounts) = (Arc::clone(&w), Arc::clone(&exports), Arc::clone(&mounts));
        let reg = reg("QNFSMNTD");
        threads.push(std::thread::spawn(move || {
            let c = Common {
                listeners: listeners(),
                binder: reg,
                stop: &stop,
                ready: Some(&ready),
            };
            run::mountd(c, &exports, &mounts, || w.vfs()).unwrap();
        }));
    }
    {
        let (stop, ready) = (Arc::clone(&stop), ready.clone());
        let (w, exports, st) = (Arc::clone(&w), Arc::clone(&exports), Arc::clone(&nlm_state));
        let reg = reg("QNFSNLMD");
        threads.push(std::thread::spawn(move || {
            let c = Common {
                listeners: listeners(),
                binder: reg,
                stop: &stop,
                ready: Some(&ready),
            };
            run::nlmd(
                c,
                &exports,
                &st,
                || w.vfs(),
                || -> Box<dyn NewsSource> { Box::new(ClientNews(w.job(1))) },
            )
            .unwrap();
        }));
    }
    {
        let (stop, ready) = (Arc::clone(&stop), ready.clone());
        let st = Arc::clone(&nsm_state);
        let reg = reg("QNFSNSMD");
        threads.push(std::thread::spawn(move || {
            let c = Common {
                listeners: nsm_l,
                binder: reg,
                stop: &stop,
                ready: Some(&ready),
            };
            run::nsmd(c, &st, Vec::new()).unwrap();
        }));
    }
    let end = Instant::now() + Duration::from_secs(20);
    while started.load(Ordering::SeqCst) < 4 {
        assert!(Instant::now() < end, "the daemons did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
    Server {
        pmap_port: baddr.port(),
        stop,
        threads,
        binder_stop,
        binder,
    }
}
