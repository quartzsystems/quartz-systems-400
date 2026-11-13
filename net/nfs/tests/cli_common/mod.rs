// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the client tests share: a server system on loopback (a binder
//! and the `mountd`, `nfsd`, `nlmd` loops over `IfsVfs` of an IFS in
//! memory) that stops, starts again on the same ports, and restarts
//! (a new IFS over the same volume, a new write verifier); a black hole
//! that takes calls and never answers; and the client's environment.

#![allow(dead_code)]

#[path = "../srv3_common/mod.rs"]
pub mod srv3_common;

use std::io::Read;
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ifs::remote::Cred;
use nfs::client::ClientEnv;
use nfs::mount::list::MountList;
use nfs::nlm::state::{NlmConfig, NlmState};
use nfs::registry::{MemIdMap, Registry};
use nfs::rpc::{ClientConfig, Dispatcher, serve_datagram};
use nfs::rpcb::Binder;
use nfs::server::SystemClock;
use nfs::server::locate::FixedLocator;
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common, Listeners, Registration};
use nfs::server::{Clock, ExportTable, ManualClock, MemExport};
use nfs::v4::cb::NoCallbacks;
use nfs::v4::state::stable::MemStable;
use nfs::v4::{Config as V4Config, Server};
pub use srv3_common::World;

/// A binder on a UDP port of its own.
pub struct BinderSrv {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    t: Option<JoinHandle<()>>,
}

impl BinderSrv {
    pub fn start() -> Self {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let addr = s.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let st = Arc::clone(&stop);
        let t = std::thread::spawn(move || {
            let mut d = Dispatcher::new();
            d.add(Arc::new(Binder::default()));
            while !st.load(Ordering::Relaxed) {
                let _ = serve_datagram(&s, &d);
            }
        });
        Self {
            addr,
            stop,
            t: Some(t),
        }
    }
}

impl Drop for BinderSrv {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.t.take() {
            let _ = t.join();
        }
    }
}

/// The ports a server system listens on (kept across its restarts).
#[derive(Debug, Clone, Copy)]
pub struct Ports {
    pub nfs: u16,
    pub mount: u16,
    pub nlm: u16,
}

struct Running {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

/// The server system.
pub struct ServerSys {
    pub world: Mutex<Arc<World>>,
    pub binder: Option<BinderSrv>,
    pub ports: Ports,
    pub nlm: Mutex<Option<Arc<NlmState>>>,
    grace: Duration,
    running: Mutex<Option<Running>>,
    /// The `NFSv4` server (`nfsd4`, versions 3 and 4 on one port), if run.
    pub v4: Mutex<Option<Arc<Server>>>,
    /// The v4 server's clock and stable records (kept across restarts).
    pub clock: Mutex<Arc<ManualClock>>,
    stable: Arc<MemStable>,
}

fn tcp(port: u16) -> TcpListener {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(l) => return l,
            Err(e) => {
                assert!(Instant::now() < end, "port {port}: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn udp(port: u16) -> UdpSocket {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        match UdpSocket::bind(("127.0.0.1", port)) {
            Ok(u) => return u,
            Err(e) => {
                assert!(Instant::now() < end, "port {port}: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

fn free_port() -> u16 {
    // A port both TCP and UDP can have.
    loop {
        let t = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = t.local_addr().unwrap().port();
        if UdpSocket::bind(("127.0.0.1", p)).is_ok() {
            return p;
        }
    }
}

impl ServerSys {
    /// A server exporting `/home/exp` (ID 1), with a binder or none,
    /// with a lock manager or none.
    pub fn start(with_binder: bool, with_nlm: bool) -> Arc<Self> {
        Self::start_with(with_binder, with_nlm, Duration::ZERO)
    }

    /// The same, the lock manager with a grace period after each start.
    pub fn start_with(with_binder: bool, with_nlm: bool, grace: Duration) -> Arc<Self> {
        let w = World::new();
        let mut e = w.export(1, "/home/exp");
        e.anon = nfs::server::Anon::Uid(srv3_common::ANON_UID);
        w.put(e);
        let s = Arc::new(Self {
            world: Mutex::new(Arc::new(w)),
            binder: with_binder.then(BinderSrv::start),
            ports: Ports {
                nfs: free_port(),
                mount: free_port(),
                nlm: free_port(),
            },
            nlm: Mutex::new(with_nlm.then(|| Self::nlm_state(Duration::ZERO))),
            grace,
            running: Mutex::new(None),
            v4: Mutex::new(None),
            clock: Mutex::new(Arc::new(ManualClock::default())),
            stable: Arc::new(MemStable::new()),
        });
        s.run();
        s
    }

    /// A server with `NFSv4` too (`nfsd4` serves versions 3 and 4); its
    /// grace period after a restart is `grace`, on a clock moved by
    /// [`Self::advance`].
    pub fn start_v4(with_binder: bool, with_nlm: bool, grace: Duration) -> Arc<Self> {
        let s = Self::start_with(with_binder, with_nlm, grace);
        s.stop();
        let srv = s.v4_server();
        *s.v4.lock().unwrap_or_else(PoisonError::into_inner) = Some(srv);
        s.run();
        s
    }

    fn v4_server(&self) -> Arc<Server> {
        let w = self.world();
        let clock = Arc::clone(&self.clock.lock().unwrap_or_else(PoisonError::into_inner));
        let reg = Arc::new(Registry::new(
            Arc::new(ids()),
            nfs::registry::TTL,
            clock.clone() as Arc<dyn Clock>,
        ));
        Server::new(
            V4Config {
                grace: self.grace,
                lease: Duration::from_secs(300),
                ..V4Config::default()
            },
            Arc::clone(&w.exports) as Arc<dyn ExportTable>,
            Arc::clone(&w.instance),
            clock as Arc<dyn Clock>,
            reg,
            self.stable.clone(),
            Arc::new(NoCallbacks),
        )
    }

    /// Moves the v4 server's clock on (ends its grace period).
    pub fn advance(&self, d: Duration) {
        self.clock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .advance(d);
    }

    fn nlm_state(grace: Duration) -> Arc<NlmState> {
        NlmState::new(NlmConfig {
            monitor: false,
            locator: Arc::new(FixedLocator::new()),
            rpc: ClientConfig {
                timeout: Duration::from_millis(300),
                retransmits: 1,
                reserved_port: false,
                ..ClientConfig::default()
            },
            grace,
            grant_tries: 1,
            clock: Arc::new(SystemClock),
        })
    }

    pub fn world(&self) -> Arc<World> {
        Arc::clone(&self.world.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Where the binder listens (a closed port without one).
    pub fn binder_port(&self) -> u16 {
        self.binder.as_ref().map_or(1, |b| b.addr.port())
    }

    fn reg(&self, owner: &str) -> Option<Registration> {
        self.binder.as_ref().map(|b| Registration {
            binder: b.addr,
            owner: owner.into(),
        })
    }

    /// Start the daemons on the system's ports.
    #[allow(clippy::too_many_lines)]
    pub fn run(&self) {
        let stop = Arc::new(AtomicBool::new(false));
        let ready = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        let w = self.world();
        let mut want = 2;
        {
            let (w, stop, ready) = (Arc::clone(&w), Arc::clone(&stop), Arc::clone(&ready));
            let reg = self.reg("QNFSMNTD");
            let (t, u) = (tcp(self.ports.mount), udp(self.ports.mount));
            threads.push(std::thread::spawn(move || {
                let on = || {
                    ready.fetch_add(1, Ordering::SeqCst);
                };
                let exports: Arc<dyn ExportTable> = Arc::clone(&w.exports) as Arc<dyn ExportTable>;
                let mounts: Arc<dyn MountList> = Arc::clone(&w.mounts) as Arc<dyn MountList>;
                let c = Common {
                    listeners: Listeners {
                        tcp: Some(t),
                        udp: Some(u),
                    },
                    binder: reg,
                    stop: &stop,
                    ready: Some(&on),
                };
                run::mountd(c, &exports, &mounts, || w.vfs()).unwrap();
            }));
        }
        {
            let (w, stop, ready) = (Arc::clone(&w), Arc::clone(&stop), Arc::clone(&ready));
            let reg = self.reg("QNFSNFSD");
            let v4 = self
                .v4
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let (t, u) = (tcp(self.ports.nfs), udp(self.ports.nfs));
            threads.push(std::thread::spawn(move || {
                let on = || {
                    ready.fetch_add(1, Ordering::SeqCst);
                };
                let c = Common {
                    listeners: Listeners {
                        tcp: Some(t),
                        udp: Some(u),
                    },
                    binder: reg,
                    stop: &stop,
                    ready: Some(&on),
                };
                match v4 {
                    Some(srv) => {
                        let w2 = Arc::clone(&w);
                        run::nfsd4(
                            c,
                            &srv,
                            || w.vfs(),
                            move || Box::new(ClientNews(w2.job(1))) as Box<dyn NewsSource>,
                        )
                        .unwrap();
                    }
                    None => run::nfsd(c, || w.nfs3()).unwrap(),
                }
            }));
        }
        let nlm = self
            .nlm
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(st) = &nlm {
            want += 1;
            let (w, stop, ready, st) = (
                Arc::clone(&w),
                Arc::clone(&stop),
                Arc::clone(&ready),
                Arc::clone(st),
            );
            let reg = self.reg("QNFSNLMD");
            let (t, u) = (tcp(self.ports.nlm), udp(self.ports.nlm));
            threads.push(std::thread::spawn(move || {
                let on = || {
                    ready.fetch_add(1, Ordering::SeqCst);
                };
                let exports: Arc<dyn ExportTable> = Arc::clone(&w.exports) as Arc<dyn ExportTable>;
                let c = Common {
                    listeners: Listeners {
                        tcp: Some(t),
                        udp: Some(u),
                    },
                    binder: reg,
                    stop: &stop,
                    ready: Some(&on),
                };
                let w2 = Arc::clone(&w);
                run::nlmd(
                    c,
                    &exports,
                    &st,
                    || w.vfs(),
                    move || -> Box<dyn NewsSource> { Box::new(ClientNews(w2.job(1))) },
                )
                .unwrap();
            }));
        }
        let end = Instant::now() + Duration::from_secs(10);
        while ready.load(Ordering::SeqCst) < want {
            assert!(Instant::now() < end, "the daemons did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
        *self.running.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Running { stop, threads });
    }

    /// Stop every daemon (their connections close).
    pub fn stop(&self) {
        let r = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(r) = r {
            r.stop.store(true, Ordering::SeqCst);
            for t in r.threads {
                let _ = t.join();
            }
        }
    }

    /// The system restarts: the daemons stop, the IFS is made again
    /// over the same volume (its locks gone, a new write verifier), the
    /// lock manager starts its grace period, and the daemons start on
    /// the same ports.
    pub fn restart(&self) {
        self.stop();
        let w = self.world().restarted();
        *self.world.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(w);
        let mut n = self.nlm.lock().unwrap_or_else(PoisonError::into_inner);
        if n.is_some() {
            *n = Some(Self::nlm_state(self.grace));
        }
        drop(n);
        let had = self
            .v4
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some();
        if had {
            *self.clock.lock().unwrap_or_else(PoisonError::into_inner) =
                Arc::new(ManualClock::default());
            let srv = self.v4_server();
            *self.v4.lock().unwrap_or_else(PoisonError::into_inner) = Some(srv);
        }
        self.run();
    }

    /// Exports `path` with `id`, as `f` changes it.
    pub fn export(&self, id: u32, path: &str, f: impl FnOnce(MemExport) -> MemExport) {
        let w = self.world();
        let e = f(w.export(id, path));
        w.put(e);
    }

    /// The client's environment for this server (no local monitor).
    pub fn env(&self, host: &str) -> ClientEnv {
        ClientEnv {
            host_name: host.into(),
            binder_port: self.binder_port(),
            reserved_port: false,
            ..ClientEnv::default()
        }
    }

    /// Options naming the ports (the binder is not asked).
    pub fn ports_opts(&self) -> String {
        format!("port={},mountport={}", self.ports.nfs, self.ports.mount)
    }
}

impl Drop for ServerSys {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A TCP and UDP port that take calls and never answer.
pub struct BlackHole {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl BlackHole {
    pub fn on(port: u16) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let t = tcp(port);
        t.set_nonblocking(true).unwrap();
        let u = udp(port);
        u.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
        let (s1, s2) = (Arc::clone(&stop), Arc::clone(&stop));
        let a = std::thread::spawn(move || {
            let mut conns = Vec::new();
            while !s1.load(Ordering::Relaxed) {
                if let Ok((c, _)) = t.accept() {
                    c.set_read_timeout(Some(Duration::from_millis(20))).ok();
                    conns.push(c);
                }
                let mut buf = [0u8; 4096];
                for c in &mut conns {
                    let _ = c.read(&mut buf);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let b = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65_536];
            while !s2.load(Ordering::Relaxed) {
                let _ = u.recv(&mut buf);
            }
        });
        Self {
            stop,
            threads: vec![a, b],
        }
    }
}

impl Drop for BlackHole {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// `BOB`'s credentials on the wire.
pub fn bob() -> Cred {
    Cred {
        uid: 100,
        gid: 100,
        gids: vec![100],
    }
}

/// UID 0 (squashed by the server unless `ROOT=`).
pub fn root() -> Cred {
    Cred {
        uid: 0,
        gid: 0,
        gids: vec![],
    }
}

/// The identity map of the server world's profiles.
pub fn ids() -> MemIdMap {
    MemIdMap {
        domain: "example.com".into(),
        users: vec![
            ("QSECOFR".into(), 0),
            ("BOB".into(), 100),
            ("ZED".into(), 102),
            ("ANN".into(), 103),
            ("QNFSANON".into(), srv3_common::ANON_UID),
        ],
        groups: vec![("DEV".into(), 500)],
    }
}
