// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The daemon jobs over a registry double and an IFS in memory, serving
//! real RPC over loopback: the binder and `/etc/RPCBTAB`, a mount and a
//! read through `QNFSMNTD` and two `QNFSNFSD` jobs (the relay a queue),
//! the export table read again when its generation moves, the status
//! monitor's files, a lock released by an `RLSIFSLCK` order, and the
//! lifecycle's ends.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use ifs::ops::{Config, Ifs, OpenArgs, Owner};
use ifscmd::Msg;
use nfs::exports::{NoNames, Resolver};
use nfs::rpc::{ClientConfig, Credential, Proto, RpcClient};
use nfs::rpcb::BinderClient;
use nfs::server::FileHandle;
use nfs::server::locate::{self, Locator};
use nfs::types::mount::MOUNT_PROGRAM::MOUNT_V3;
use nfs::types::mount::{dirpath, mountres3};
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3;
use nfs::types::nfs3::{self as t3};
use nfs::types::nlm::NLM_PROG::NLM4_VERS;
use nfs::types::nlm::{netobj, nlm4_lock, nlm4_lockargs, nlm4_stats};
use qir::MsgType;
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::nfs::{Daemon, NfsDaemonJob, NfsExport, NfsOp, NfsOptions, NfsOrder, NfsReply};

use super::host::{Host, Net};
use super::running::{Fail, Running};
use super::{DaemonEnv, Serve, Started, lifecycle, rpcd, serve_with, statd};

const ANON_UID: u32 = 4_294_770_710;

fn qsecofr() -> Principal {
    Principal {
        user: "QSECOFR".into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        spcaut: SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0 | SpecialAuth::SECADM.0,
        ..Principal::default()
    }
}

/// The registry and the export table of the service, as a double.
#[derive(Default)]
struct Svc {
    jobs: Vec<(u32, NfsDaemonJob)>,
    generation: u64,
    exports: Vec<NfsExport>,
    restore: bool,
    orders: Vec<NfsOrder>,
    done: Vec<(u32, u32, i32)>,
    failed: Vec<u32>,
    mapping: bool,
    restarted: Vec<(String, i32)>,
}

impl Svc {
    fn answer(&mut self, number: u32, op: NfsOp) -> NfsReply {
        match op {
            NfsOp::Hello { daemon } => {
                let mut index = 1;
                while self
                    .jobs
                    .iter()
                    .any(|(_, j)| j.daemon == daemon && j.index == index)
                {
                    index += 1;
                }
                self.jobs.push((
                    number,
                    NfsDaemonJob {
                        daemon,
                        index,
                        job: format!("{number:06}/QSECOFR/{}", daemon.job_name()),
                        ready: false,
                        ending: false,
                        ports: Vec::new(),
                        since: 0,
                    },
                ));
                NfsReply::Hello {
                    index,
                    restore: daemon == Daemon::Rpc && std::mem::take(&mut self.restore),
                }
            }
            NfsOp::Ready { ports } => {
                for (n, j) in &mut self.jobs {
                    if *n == number {
                        j.ready = true;
                        j.ports.clone_from(&ports);
                    }
                }
                NfsReply::Done
            }
            NfsOp::Gone => {
                self.jobs.retain(|(n, _)| *n != number);
                NfsReply::Done
            }
            NfsOp::Failed { .. } => {
                self.jobs.retain(|(n, _)| *n != number);
                self.failed.push(number);
                NfsReply::Done
            }
            NfsOp::ShouldEnd => NfsReply::Flag {
                yes: self
                    .jobs
                    .iter()
                    .find(|(n, _)| *n == number)
                    .is_none_or(|(_, j)| j.ending),
            },
            NfsOp::Daemons => NfsReply::Daemons {
                jobs: self.jobs.iter().map(|(_, j)| j.clone()).collect(),
            },
            NfsOp::Generation => NfsReply::Generation {
                generation: self.generation,
            },
            NfsOp::Exports => NfsReply::Table {
                generation: self.generation,
                exports: self.exports.clone(),
            },
            NfsOp::Options => NfsReply::Options {
                options: NfsOptions::default(),
            },
            NfsOp::UidOfName { name } => NfsReply::Id {
                id: match name.as_str() {
                    "QSYS" => Some(101),
                    "BOB" => Some(100),
                    _ => None,
                },
            },
            NfsOp::NameOfUid { uid } => NfsReply::Name {
                name: (uid == 100).then(|| "BOB".to_string()),
            },
            NfsOp::Mapping => NfsReply::Mapping {
                enabled: self.mapping,
                domain: "lab.example".into(),
            },
            NfsOp::ServerRestarted { name, state } => {
                self.restarted.push((name, state));
                NfsReply::Done
            }
            NfsOp::Orders => NfsReply::Orders {
                orders: std::mem::take(&mut self.orders),
            },
            NfsOp::OrderDone { id, count, errno } => {
                self.done.push((id, count, errno));
                NfsReply::Done
            }
            _ => NfsReply::Done,
        }
    }

    fn end(&mut self, number: u32) {
        for (n, j) in &mut self.jobs {
            if *n == number {
                j.ending = true;
            }
        }
    }

    fn job(&self, number: u32) -> Option<NfsDaemonJob> {
        self.jobs
            .iter()
            .find(|(n, _)| *n == number)
            .map(|(_, j)| j.clone())
    }
}

/// What every job of a test shares: the IFS, the service, the relay.
struct World {
    ifs: Arc<Ifs<MemBackend>>,
    env: MemEnv,
    svc: Mutex<Svc>,
    binder: Mutex<Option<SocketAddr>>,
    relay: Mutex<HashMap<u16, VecDeque<TcpStream>>>,
    relayed: AtomicUsize,
}

impl World {
    fn new() -> Arc<Self> {
        let env = MemEnv::default()
            .user("QSECOFR", 0)
            .user("QSYS", 101)
            .user("BOB", 100)
            .user("QNFSANON", ANON_UID)
            .user("QDFTOWN", 102)
            .principal(qsecofr());
        let ifs = Ifs::new(
            MemBackend::new(),
            Box::new(ifs::vfs::qsys::MemQsys::new(sec::auth::bits::Auth::USE)),
            Config::default(),
        );
        ifs.ensure_layout(&env).unwrap();
        let w = Arc::new(Self {
            ifs: Arc::new(ifs),
            env,
            svc: Mutex::default(),
            binder: Mutex::new(None),
            relay: Mutex::default(),
            relayed: AtomicUsize::new(0),
        });
        let c = w.client(99);
        for d in [
            "/etc",
            "/home",
            "/QIBM",
            "/QIBM/UserData",
            "/QIBM/UserData/OS400",
        ] {
            let _ = c.mkdir(d, 0o755);
        }
        w
    }

    fn client(&self, owner: u64) -> LocalIfs<MemBackend> {
        LocalIfs {
            ifs: Arc::clone(&self.ifs),
            env: Arc::new(self.env.clone()),
            audit: Arc::new(MemAudit::default()),
            principal: qsecofr(),
            owner: Owner(owner),
            job_ccsid: 37,
        }
    }

    fn svc(&self) -> std::sync::MutexGuard<'_, Svc> {
        self.svc.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn export(&self, id: u32, path: &str) {
        let c = self.client(99);
        let _ = c.mkdir(path, 0o777);
        let _ = c.chmod(path, 0o777);
        let mut s = self.svc();
        s.exports.push(NfsExport {
            id,
            entry: path.into(),
            anon_uid: Some(ANON_UID),
            anon_profile: "QNFSANON".into(),
            by: "000001/QSECOFR/QPADEV0001".into(),
            since: 0,
        });
        s.generation += 1;
    }

    fn put(&self, path: &str, data: &[u8]) {
        let c = self.client(99);
        let fd = c
            .open(
                path,
                OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o666),
            )
            .unwrap();
        ifs::client::write_all(&c, fd, data).unwrap();
        c.close(fd).unwrap();
        // Every user may write it (the umask narrowed the mode).
        c.chmod(path, 0o666).unwrap();
    }

    fn get(&self, path: &str) -> Option<Vec<u8>> {
        let c = self.client(99);
        let fd = c.open(path, OpenArgs::new(O_RDONLY)).ok()?;
        let d = ifs::client::read_all(&c, fd).ok();
        let _ = c.close(fd);
        d
    }

    fn fid(&self, path: &str) -> FileId {
        let s = self.client(99).stat(path, true).unwrap();
        FileId {
            fs: s.vfs,
            ino: s.ino,
            generation: s.ino_gen,
        }
    }
}

/// Finds programs through the test's binder.
struct TestLocator(SocketAddr);

impl Locator for TestLocator {
    fn find(&self, _host: &locate::Host, prog: u32, vers: u32, proto: Proto) -> Option<SocketAddr> {
        let mut b = BinderClient::at(self.0, Proto::Udp, cfg());
        let p = if proto == Proto::Tcp { 6 } else { 17 };
        let port = b.getport(prog, vers, p).ok()??;
        Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
    }
}

/// One daemon job's host.
struct TestHost {
    w: Arc<World>,
    number: u32,
    index: Mutex<u16>,
}

impl Host for TestHost {
    type Fs = LocalIfs<MemBackend>;

    fn fs(&self) -> Result<Self::Fs, String> {
        Ok(self.w.client(u64::from(self.number)))
    }
    fn op(&self, op: NfsOp) -> NfsReply {
        self.w.svc().answer(self.number, op)
    }
    fn net(&self) -> Net {
        Net {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            binder: self
                .w
                .binder
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .unwrap_or_else(|| SocketAddr::from((Ipv4Addr::LOCALHOST, 1))),
            rpc_port: 0,
            nfs_port: 0,
        }
    }
    fn give(&self, to: &NfsDaemonJob, s: TcpStream) -> Result<(), TcpStream> {
        self.w.relayed.fetch_add(1, Ordering::SeqCst);
        self.w
            .relay
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(to.index)
            .or_default()
            .push_back(s);
        Ok(())
    }
    fn take(&self, ms: u32) -> Option<TcpStream> {
        let me = *self.index.lock().unwrap_or_else(PoisonError::into_inner);
        let end = Instant::now() + Duration::from_millis(u64::from(ms));
        loop {
            if let Some(s) = self
                .w
                .relay
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_mut(&me)
                .and_then(VecDeque::pop_front)
            {
                return Some(s);
            }
            if Instant::now() >= end {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn names(&self) -> Box<dyn Resolver + Send + Sync> {
        Box::new(NoNames)
    }
    fn locator(&self) -> Arc<dyn Locator> {
        Arc::new(TestLocator(self.net().binder))
    }
    fn host_name(&self) -> String {
        "qs400test".into()
    }
    fn system_name(&self) -> String {
        "QS400TST".into()
    }
}

/// A job's environment: its log, and a flag that ends it.
struct TestEnv {
    host: Arc<TestHost>,
    log: Arc<Mutex<Vec<String>>>,
    end: Arc<AtomicBool>,
}

impl DaemonEnv for TestEnv {
    fn op(&mut self, op: NfsOp) -> NfsReply {
        self.host.op(op)
    }
    fn idle(&mut self) -> bool {
        std::thread::sleep(Duration::from_millis(20));
        !self.end.load(Ordering::SeqCst)
    }
    fn log(&mut self, m: &Msg, _: MsgType) {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(m.id.clone());
    }
}

/// A daemon job running on a thread.
struct Job {
    number: u32,
    end: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<u16>>,
}

static NUMBERS: AtomicUsize = AtomicUsize::new(100);

impl Job {
    /// Start `daemon` and wait until it is ready.
    fn start(w: &Arc<World>, daemon: Daemon) -> Self {
        let number = u32::try_from(NUMBERS.fetch_add(1, Ordering::SeqCst)).unwrap();
        let host = Arc::new(TestHost {
            w: Arc::clone(w),
            number,
            index: Mutex::new(0),
        });
        let end = Arc::new(AtomicBool::new(false));
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut env = TestEnv {
            host: Arc::clone(&host),
            log: Arc::clone(&log),
            end: Arc::clone(&end),
        };
        let thread = std::thread::spawn(move || {
            let h = Arc::clone(&host);
            let mut serve = |e: &mut dyn DaemonEnv, s: &Started| {
                *h.index.lock().unwrap_or_else(PoisonError::into_inner) = s.index;
                serve_with(e, *s, &h)
            };
            lifecycle(&mut env, daemon, &mut serve)
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        while !w.svc().job(number).is_some_and(|j| j.ready) {
            assert!(Instant::now() < deadline, "{daemon:?} did not become ready");
            assert!(
                !thread.is_finished(),
                "{daemon:?} ended: {:?}",
                log.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        if daemon == Daemon::Rpc {
            let port = w
                .svc()
                .job(number)
                .unwrap()
                .ports
                .iter()
                .find(|p| p.proto == "udp")
                .unwrap()
                .port;
            *w.binder.lock().unwrap() = Some(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
        }
        Self {
            number,
            end,
            log,
            thread: Some(thread),
        }
    }

    /// `ENDNFSSVR`: the registry asks it to end; its end code.
    fn end(mut self, w: &World) -> (u16, Vec<String>) {
        w.svc().end(self.number);
        let code = self.thread.take().unwrap().join().unwrap();
        (code, self.log.lock().unwrap().clone())
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.end.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn cfg() -> ClientConfig {
    ClientConfig {
        timeout: Duration::from_secs(3),
        retransmits: 2,
        reserved_port: false,
        ..ClientConfig::default()
    }
}

fn binder_of(w: &World) -> BinderClient {
    BinderClient::at(w.binder.lock().unwrap().unwrap(), Proto::Udp, cfg())
}

fn at(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn mount(w: &World, path: &str) -> Result<Vec<u8>, mountres3> {
    let port = binder_of(w)
        .getport(100_005, 3, 6)
        .unwrap()
        .expect("MOUNT registered");
    let mut m = RpcClient::new(at(port), Proto::Tcp, cfg());
    let cred = Credential::sys(b"client1", 100, 100, &[]);
    match m
        .call_proc::<MOUNT_V3::Args, MOUNT_V3::Res>(
            &cred,
            &MOUNT_V3::Args::MOUNTPROC3_MNT(dirpath(path.as_bytes().to_vec())),
        )
        .unwrap()
    {
        MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::MNT3_OK(ok)) => Ok(ok.fhandle.0),
        MOUNT_V3::Res::MOUNTPROC3_MNT(r) => Err(r),
        r => panic!("{r:?}"),
    }
}

#[test]
fn the_binder_serves_and_keeps_rpcbtab() {
    let w = World::new();
    let rpc = Job::start(&w, Daemon::Rpc);
    let dump = binder_of(&w).dump().unwrap();
    let own: Vec<(u32, u32, Vec<u8>)> = dump
        .iter()
        .map(|r| (r.r_prog, r.r_vers, r.r_netid.clone()))
        .collect();
    for v in [2, 3, 4] {
        assert!(own.contains(&(100_000, v, b"tcp".to_vec())), "{own:?}");
        assert!(own.contains(&(100_000, v, b"udp".to_vec())), "{own:?}");
    }
    // The file follows the table, in the oracle's form.
    let deadline = Instant::now() + Duration::from_secs(5);
    let table = loop {
        if let Some(b) = w.get(rpcd::RPCBTAB)
            && let Ok(t) = nfs::rpcb::rpcbtab::decode(&b)
            && t.len() == dump.len()
        {
            assert_eq!(nfs::rpcb::rpcbtab::encode(&t).unwrap(), b);
            break t;
        }
        assert!(Instant::now() < deadline, "no {}", rpcd::RPCBTAB);
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        table
            .iter()
            .all(|r| r.r_owner == b"0" && r.r_addr.starts_with(b"::."))
    );
    let st = w.client(99).stat(rpcd::RPCBTAB, true).unwrap();
    // Read and write for everyone; the group's bits show only with a
    // primary group, which the file has none of.
    assert_eq!((st.ccsid, st.mode & 0o707), (37, 0o606));
    let (code, log) = rpc.end(&w);
    assert_eq!((code, log), (0, vec!["CPCA1B5".to_string()]));
    // RTVRPCREG(*YES): a row whose daemon is gone comes back, harmless.
    let mut t = table;
    t.push(nfs::types::rpcb::rpcb {
        r_prog: 100_099,
        r_vers: 1,
        r_netid: b"tcp".to_vec(),
        r_addr: b"::.4.0".to_vec(),
        r_owner: b"4294967295".to_vec(),
    });
    rpcd::write(&w.client(99), &t).unwrap();
    w.svc().restore = true;
    let _rpc = Job::start(&w, Daemon::Rpc);
    let progs: Vec<u32> = binder_of(&w)
        .dump()
        .unwrap()
        .into_iter()
        .map(|r| r.r_prog)
        .collect();
    assert!(progs.contains(&100_099), "{progs:?}");
}

#[test]
fn a_mount_and_a_read_through_the_daemons() {
    let w = World::new();
    w.export(1, "/home/exp");
    w.put("/home/exp/data.txt", b"hello over NFS");
    let _rpc = Job::start(&w, Daemon::Rpc);
    let nfs1 = Job::start(&w, Daemon::Svr);
    let nfs2 = Job::start(&w, Daemon::Svr);
    let mnt = Job::start(&w, Daemon::Mnt);
    let root = mount(&w, "/home/exp").expect("mounted");
    assert_eq!(
        mount(&w, "/home/exp2").unwrap_err(),
        mountres3::default(nfs::types::mount::mountstat3::MNT3ERR_ACCES)
    );
    let port = binder_of(&w)
        .getport(100_003, 3, 6)
        .unwrap()
        .expect("NFS registered");
    let mut n = RpcClient::new(at(port), Proto::Tcp, cfg());
    let cred = Credential::sys(b"client1", 100, 100, &[]);
    let found = n
        .call_proc::<NFS_V3::Args, NFS_V3::Res>(
            &cred,
            &NFS_V3::Args::NFSPROC3_LOOKUP(t3::LOOKUP3args {
                what: t3::diropargs3 {
                    dir: t3::nfs_fh3 { data: root.clone() },
                    name: t3::filename3(b"data.txt".to_vec()),
                },
            }),
        )
        .unwrap();
    let fh = match found {
        NFS_V3::Res::NFSPROC3_LOOKUP(t3::LOOKUP3res::NFS3_OK(ok)) => ok.object,
        r => panic!("{r:?}"),
    };
    let read = n
        .call_proc::<NFS_V3::Args, NFS_V3::Res>(
            &cred,
            &NFS_V3::Args::NFSPROC3_READ(t3::READ3args {
                file: fh,
                offset: 0,
                count: 4096,
            }),
        )
        .unwrap();
    match read {
        NFS_V3::Res::NFSPROC3_READ(t3::READ3res::NFS3_OK(ok)) => {
            assert_eq!(ok.data, b"hello over NFS");
            assert!(ok.eof);
        }
        r => panic!("{r:?}"),
    }
    // The connection went where its address hashes among the two jobs.
    let second = super::nfsd::pick(IpAddr::V4(Ipv4Addr::LOCALHOST), 2) == 1;
    assert_eq!(w.relayed.load(Ordering::SeqCst), usize::from(second));
    // A new export is served once its generation is seen.
    w.export(2, "/home/exp2");
    let deadline = Instant::now() + Duration::from_secs(5);
    while mount(&w, "/home/exp2").is_err() {
        assert!(Instant::now() < deadline, "the table was not read again");
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(n);
    // Each ends, its registrations taken back.
    assert_eq!(mnt.end(&w).0, 0);
    assert_eq!(nfs2.end(&w).0, 0);
    let (code, log) = nfs1.end(&w);
    assert_eq!((code, log), (0, vec!["CPCA1B2".to_string()]));
    let left: Vec<u32> = binder_of(&w)
        .dump()
        .unwrap()
        .into_iter()
        .map(|r| r.r_prog)
        .collect();
    assert!(
        !left.contains(&100_003) && !left.contains(&100_005),
        "{left:?}"
    );
}

#[test]
fn the_monitor_keeps_its_files_and_the_lock_manager_takes_orders() {
    let w = World::new();
    w.export(1, "/home/locks");
    w.put("/home/locks/f", b"0123456789");
    let _rpc = Job::start(&w, Daemon::Rpc);
    let nsm = Job::start(&w, Daemon::Nsm);
    assert_eq!(w.get(statd::STATD).unwrap(), [0xF1, 0x15]);
    let st = w.client(99).stat(statd::STATD, true).unwrap();
    assert_eq!(
        (st.mode & 0o777, st.ccsid, st.owner.as_str()),
        (0o600, 37, "QSYS")
    );
    let nlm = Job::start(&w, Daemon::Nlm);
    let port = binder_of(&w)
        .getport(100_021, 4, 6)
        .unwrap()
        .expect("NLM registered");
    let root = w.fid("/home/locks");
    let fh = FileHandle {
        export: 1,
        fid: w.fid("/home/locks/f"),
        root_ino: root.ino,
    }
    .to_bytes()
    .to_vec();
    let mut c = RpcClient::new(at(port), Proto::Tcp, cfg());
    let cred = Credential::sys(b"client1", 100, 100, &[]);
    // A reclaim is taken in the grace period of the start.
    let r = c
        .call_proc::<NLM4_VERS::Args, NLM4_VERS::Res>(
            &cred,
            &NLM4_VERS::Args::NLMPROC4_LOCK(nlm4_lockargs {
                cookie: netobj(b"c".to_vec()),
                block: false,
                exclusive: true,
                alock: nlm4_lock {
                    caller_name: b"client1".to_vec(),
                    fh: netobj(fh),
                    oh: netobj(b"owner".to_vec()),
                    svid: 7,
                    l_offset: 0,
                    l_len: 4,
                },
                reclaim: true,
                state: 1,
            }),
        )
        .unwrap();
    match r {
        NLM4_VERS::Res::NLMPROC4_LOCK(x) => assert_eq!(x.stat.stat, nlm4_stats::NLM4_GRANTED),
        r => panic!("{r:?}"),
    }
    // The monitor was asked to watch the host, and keeps it on disk.
    let mon = w.get(statd::MONITORED).unwrap();
    let hosts = nfs::nsm::record::decode_monitored(&mon).unwrap();
    assert!(hosts.iter().any(|h| h.mon_name == b"client1"), "{hosts:?}");
    // RLSIFSLCK RMTLOCNAME(client1): the order, taken at a pause.
    w.svc().orders.push(NfsOrder {
        id: 5,
        host: "client1".into(),
        by: "000001/QSECOFR/QPADEV0001".into(),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while w.svc().done.is_empty() {
        assert!(Instant::now() < deadline, "the order was not carried out");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(w.svc().done[0], (5, 1, 0));
    assert!(nlm.log.lock().unwrap().contains(&"CPCA1C9".to_string()));
    assert_eq!(nlm.end(&w).0, 0);
    // A start after an end moves the state to the next odd number.
    assert_eq!(nsm.end(&w).0, 0);
    let _nsm = Job::start(&w, Daemon::Nsm);
    assert_eq!(w.get(statd::STATD).unwrap(), [0xF3, 0x15]);
}

#[test]
fn helpers_and_the_registry_daemon_are_ready_and_end() {
    let w = World::new();
    for d in [Daemon::Bio, Daemon::Rgy] {
        let j = Job::start(&w, d);
        assert!(w.svc().job(j.number).unwrap().ports.is_empty());
        assert_eq!(j.end(&w), (0, vec!["CPCA1B2".to_string()]));
    }
}

/// The lifecycle over a bare environment.
struct Bare {
    ops: Vec<String>,
    log: Vec<String>,
    idles: u32,
    end_after: u32,
}

impl DaemonEnv for Bare {
    fn op(&mut self, op: NfsOp) -> NfsReply {
        let name = format!("{op:?}");
        self.ops
            .push(name.split([' ', '{']).next().unwrap_or("").to_string());
        match op {
            NfsOp::Hello { .. } => NfsReply::Hello {
                index: 1,
                restore: false,
            },
            NfsOp::ShouldEnd => NfsReply::Flag {
                yes: self.idles >= self.end_after,
            },
            _ => NfsReply::Done,
        }
    }
    fn idle(&mut self) -> bool {
        self.idles += 1;
        std::thread::sleep(Duration::from_millis(20));
        true
    }
    fn log(&mut self, m: &Msg, _: MsgType) {
        self.log.push(m.id.clone());
    }
}

fn bare() -> Bare {
    Bare {
        ops: Vec::new(),
        log: Vec::new(),
        idles: 0,
        end_after: 1000,
    }
}

#[test]
fn a_daemon_that_cannot_start_says_why_and_ends() {
    for (d, want) in [
        (Daemon::Svr, vec!["CPE3447", "CPFA1BC"]),
        (Daemon::Rpc, vec!["CPE3447", "CPFA1CC", "CPFA1BC"]),
    ] {
        let mut e = bare();
        let mut serve = |_: &mut dyn DaemonEnv, _: &Started| {
            Serve::Unavailable(Fail {
                errno: 98,
                what: "port in use".into(),
            })
        };
        assert_eq!(lifecycle(&mut e, d, &mut serve), 20);
        let ids: Vec<&str> = e.log.iter().map(String::as_str).collect();
        assert_eq!(ids.len(), want.len(), "{ids:?}");
        assert_eq!(&ids[1..], &want[1..]);
        assert!(!e.ops.contains(&"Ready".to_string()));
        assert_eq!(e.ops.last().map(String::as_str), Some("Gone"));
    }
}

#[test]
fn a_loop_that_stops_by_itself_is_reported() {
    for (d, want) in [(Daemon::Nlm, "CPFA1B5"), (Daemon::Rpc, "CPFA1CB")] {
        let mut e = bare();
        let mut serve = |_: &mut dyn DaemonEnv, _: &Started| {
            Serve::Ready(
                Running::spawn("t", |_, ready| {
                    ready();
                    std::thread::sleep(Duration::from_millis(50));
                    Err(Fail::new("the socket went"))
                })
                .unwrap(),
            )
        };
        assert_eq!(lifecycle(&mut e, d, &mut serve), 30);
        assert!(e.log.contains(&want.to_string()), "{:?}", e.log);
        assert_eq!(e.ops.last().map(String::as_str), Some("Failed"));
    }
}

#[test]
fn a_daemon_asked_to_end_ends_its_loop() {
    let mut e = bare();
    e.end_after = 3;
    let stopped = Arc::new(AtomicBool::new(false));
    let s2 = Arc::clone(&stopped);
    let mut serve = move |_: &mut dyn DaemonEnv, _: &Started| {
        let s3 = Arc::clone(&s2);
        Serve::Ready(
            Running::spawn("t", |stop, ready| {
                ready();
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            })
            .unwrap()
            .on_end(move || s3.store(true, Ordering::SeqCst)),
        )
    };
    assert_eq!(lifecycle(&mut e, Daemon::Mnt, &mut serve), 0);
    assert!(stopped.load(Ordering::SeqCst), "the end actions ran");
    assert_eq!(e.ops.first().map(String::as_str), Some("Hello"));
    assert_eq!(e.ops.get(1).map(String::as_str), Some("Ready"));
    assert_eq!(e.ops.last().map(String::as_str), Some("Gone"));
    assert_eq!(e.log, ["CPCA1B2"]);
}

fn bob() -> ifs::remote::Cred {
    ifs::remote::Cred {
        uid: 100,
        gid: 100,
        gids: vec![100],
    }
}

fn client_env(w: &World) -> nfs::client::ClientEnv {
    nfs::client::ClientEnv {
        host_name: "client1".into(),
        binder_port: w.binder.lock().unwrap().unwrap().port(),
        reserved_port: false,
        ..nfs::client::ClientEnv::default()
    }
}

/// The server jobs' records of `NFSv4` clients, every job's.
fn stable_clients(w: &World) -> Vec<nfs::v4::state::stable::StableClient> {
    let mut out = Vec::new();
    for n in 1..=2 {
        if let Some(b) = w.get(&nfs::v4::state::stable::file_name(n))
            && let Ok(img) = nfs::v4::state::stable::decode(&b)
        {
            out.extend(img.clients);
        }
    }
    out
}

#[test]
fn nfs4_through_the_server_jobs_with_its_records_across_a_restart() {
    use ifs::remote::{CreateMode, LockOwner, RemoteFs, SetAttr, Share, Stable};
    let w = World::new();
    w.export(1, "/home/exp");
    let _rpc = Job::start(&w, Daemon::Rpc);
    let s1 = Job::start(&w, Daemon::Svr);
    let s2 = Job::start(&w, Daemon::Svr);
    let mut b = binder_of(&w);
    assert!(
        b.getport(100_003, 4, 6).unwrap().is_some(),
        "version 4 registered"
    );
    let port = b.getport(100_003, 3, 6).unwrap().unwrap();
    let o = nfs::exports::mount::parse(&format!(
        "vers=4.1,soft,timeo=10,retrans=3,retry=1,port={port}"
    ))
    .unwrap();
    let m = nfs::client::connect(
        "127.0.0.1:/home/exp",
        &o,
        ifs::remote::MountCcsid::default(),
        &bob(),
        &client_env(&w),
    )
    .unwrap();
    let c = bob();
    let (root, _) = m.root().unwrap();
    let (f, _) = m
        .create(
            &c,
            &root,
            b"v4file",
            CreateMode::Guarded(SetAttr::default()),
        )
        .unwrap();
    let share = Share {
        read: true,
        write: true,
        ..Share::default()
    };
    // The open is held across the jobs' end, as a crash would leave it.
    let _open = m.open(&c, &f, share, LockOwner(1)).unwrap();
    assert_eq!(
        m.write(&c, &f, 0, b"over v4", Stable::FileSync)
            .unwrap()
            .count,
        7
    );
    assert_eq!(m.read(&c, &f, 0, 64).unwrap(), (b"over v4".to_vec(), true));
    assert_eq!(w.get("/home/exp/v4file").unwrap(), b"over v4");
    // The client holding state is recorded by the job that served it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stable_clients(&w).iter().any(|c| c.holds) {
        assert!(Instant::now() < deadline, "no stable record written");
        std::thread::sleep(Duration::from_millis(50));
    }
    // Its connections went to the job its address hashes to.
    let second = super::nfsd::pick(IpAddr::V4(Ipv4Addr::LOCALHOST), 2) == 1;
    assert_eq!(w.relayed.load(Ordering::SeqCst) > 0, second);
    drop(m);
    assert_eq!(s2.end(&w).0, 0);
    assert_eq!(s1.end(&w).0, 0);
    // A new start names it as a client that may reclaim.
    let _s1 = Job::start(&w, Daemon::Svr);
    let _s2 = Job::start(&w, Daemon::Svr);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stable_clients(&w).iter().any(|c| c.may_reclaim) {
        assert!(
            Instant::now() < deadline,
            "the restart does not name the client: {:?}",
            stable_clients(&w)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn owners_are_names_only_while_the_registry_daemon_runs() {
    let w = World::new();
    let host = Arc::new(TestHost {
        w: Arc::clone(&w),
        number: 1,
        index: Mutex::new(1),
    });
    let r = super::ids::registry(&host);
    assert_eq!(r.owner(100, None), "100");
    assert_eq!(
        r.uid(b"BOB@lab.example"),
        Err(nfs::registry::IdError::BadOwner)
    );
    w.svc().mapping = true;
    let r = super::ids::registry(&host);
    assert_eq!(r.owner(100, None), "BOB@lab.example");
    assert_eq!(r.uid(b"bob@LAB.example"), Ok(100));
    assert_eq!(r.uid(b"7"), Ok(7));
}

#[test]
fn a_block_io_helper_reads_and_writes_through_the_client() {
    use ifs::remote::{HelperOp, HelperWork};
    let w = World::new();
    w.export(1, "/home/exp");
    w.put("/home/exp/blocks", b"0123456789");
    let _rpc = Job::start(&w, Daemon::Rpc);
    let _nfs = Job::start(&w, Daemon::Svr);
    let _mnt = Job::start(&w, Daemon::Mnt);
    let mut b = binder_of(&w);
    let nport = b.getport(100_003, 3, 6).unwrap().unwrap();
    let mport = b.getport(100_005, 3, 6).unwrap().unwrap();
    let opts = format!("vers=3,proto=tcp,port={nport},mountport={mport}");
    let mut h = super::biod::Helper::new(
        Box::new(move |id: u32| {
            if id == 7 {
                Ok(opts.clone())
            } else {
                Err(ifs::Errno::ESTALE)
            }
        }),
        client_env(&w),
    );
    let fh = FileHandle {
        export: 1,
        fid: w.fid("/home/exp/blocks"),
        root_ino: w.fid("/home/exp").ino,
    }
    .to_bytes()
    .to_vec();
    let work = |op, off, len, data: &[u8]| HelperWork {
        id: 1,
        fs: 7,
        source: "127.0.0.1:/home/exp".into(),
        op,
        handle: fh.clone(),
        off,
        len,
        data: data.to_vec(),
        sync: true,
        uid: 100,
        gid: 100,
        gids: vec![100],
    };
    let r = h.run(&work(HelperOp::Read, 2, 4, b""));
    assert_eq!((r.err, r.data.as_slice()), (None, &b"2345"[..]));
    let r = h.run(&work(HelperOp::Write, 0, 2, b"ab"));
    assert_eq!((r.err, r.count, r.stable), (None, 2, true));
    assert_eq!(w.get("/home/exp/blocks").unwrap(), b"ab23456789");
    // A mount the IFS does not know is given back failed, not held.
    let mut gone = work(HelperOp::Read, 0, 4, b"");
    gone.fs = 8;
    assert_eq!(h.run(&gone).err, Some(ifs::Errno::ESTALE));
}

#[test]
fn a_restart_notice_for_the_client_reaches_the_service() {
    let w = World::new();
    let host = Arc::new(TestHost {
        w: Arc::clone(&w),
        number: 1,
        index: Mutex::new(1),
    });
    let c = super::nlmd::lock_client(&host);
    c.restarted(b"server9", 5, [0; 16]);
    assert!(w.svc().restarted.is_empty(), "not the client's");
    c.restarted(b"server9", 7, nfs::client::nlm::PRIV_TAG);
    assert_eq!(w.svc().restarted, [("server9".to_string(), 7)]);
}

#[test]
fn a_local_deny_open_after_a_v3_write_is_not_held_back() {
    let w = World::new();
    w.export(1, "/home/exp");
    w.put("/home/exp/shared", b"");
    let _rpc = Job::start(&w, Daemon::Rpc);
    let _nfs = Job::start(&w, Daemon::Svr);
    let port = binder_of(&w).getport(100_003, 3, 6).unwrap().unwrap();
    let fh = FileHandle {
        export: 1,
        fid: w.fid("/home/exp/shared"),
        root_ino: w.fid("/home/exp").ino,
    }
    .to_bytes()
    .to_vec();
    let mut n = RpcClient::new(at(port), Proto::Tcp, cfg());
    let cred = Credential::sys(b"client1", 100, 100, &[]);
    let r = n
        .call_proc::<NFS_V3::Args, NFS_V3::Res>(
            &cred,
            &NFS_V3::Args::NFSPROC3_WRITE(t3::WRITE3args {
                file: t3::nfs_fh3 { data: fh },
                offset: 0,
                count: 2,
                stable: t3::stable_how::FILE_SYNC,
                data: b"hi".to_vec(),
            }),
        )
        .unwrap();
    assert!(matches!(
        r,
        NFS_V3::Res::NFSPROC3_WRITE(t3::WRITE3res::NFS3_OK(_))
    ));
    // A local job's open that denies everyone else, with no further NFS
    // call: the server job's kept descriptor yields at once.
    let start = Instant::now();
    let fd = loop {
        match w.client(55).open(
            "/home/exp/shared",
            OpenArgs::new(ifs::ops::flags::O_RDWR | ifs::ops::flags::O_SHARE_NONE),
        ) {
            Ok(fd) => break fd,
            Err(ifs::Errno::EBUSY) if start.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => panic!("still refused after {:?}: {e:?}", start.elapsed()),
        }
    };
    assert!(start.elapsed() < Duration::from_secs(3));
    w.client(55).close(fd).unwrap();
}
