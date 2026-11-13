// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSNFSD` (`NBRSVR` jobs): program 100003, versions 3 and 4 (4.0 and
//! 4.1), one `v4::Server` a job. The first job binds TCP and UDP 2049
//! and registers with the binder; it answers UDP itself and hands each
//! TCP connection to the job its client's address hashes to among the
//! ready server jobs (the descriptor relay, as `QZLSSERVER` hands them
//! to `QZLSFILE`), so one client's v4 state stays in one job. A
//! connection whose job cannot take it, and those left queued for a job
//! that ends, are served by the first job. Each job serves each
//! connection on a thread with a link of its own (all the job's links
//! are one IFS owner) and its own file system, through
//! `server::run::connection` (the 4.1 backchannel is the connection
//! itself); the job's threads share the export table, the duplicate
//! request cache, the write verifier, the v4 state and the identity
//! mapping. A thread of its own runs the v4 server's background work
//! (leases, grace, recalls) and its lease-break news. `QZNFNFSO`'s
//! thread limit caps a job's connection threads. `RLSIFSLCK
//! RMTLOCNAME` orders take the host's v4 clients' state away.

use std::cell::OnceCell;
use std::io;
use std::net::{IpAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ifscmd::Msg;
use nfs::rpc::Dispatcher;
use nfs::server::ifsvfs::kept::JobNews;
use nfs::server::news::NewsSource;
use nfs::server::run::{self, Listeners};
use nfs::server::{ExportTable, IfsVfs, Instance, SystemClock};
use nfs::types::nfs3::NFS_PROGRAM;
use nfs::v4::cb::TcpConnector;
use nfs::v4::{Config, Server};
use qir::MsgType;
use stg::nfs::{Daemon, NfsDaemonJob, NfsOp, NfsReply};

use super::host::{Host, thread_vfs};
use super::running::{self, Fail, Prog, Running};
use super::v4stable::IfsStable;
use super::view::{self, JobExports};
use super::{DaemonEnv, Serve, Started, ids};

/// How often a waiting thread looks at the stop flag.
const TICK: Duration = Duration::from_millis(100);
/// How often the first job asks which server jobs run.
const PEERS_EVERY: Duration = Duration::from_secs(1);

/// The versions of program 100003 the server registers.
pub const VERSIONS: &[u32] = &[3, 4];

/// The server job a client address goes to: FNV-1a of the address over
/// the ready jobs, in their order.
#[must_use]
pub fn pick(addr: IpAddr, jobs: usize) -> usize {
    let bytes = match addr {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a
            .to_ipv4_mapped()
            .map_or_else(|| a.octets().to_vec(), |v4| v4.octets().to_vec()),
    };
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    usize::try_from(h % u64::try_from(jobs.max(1)).unwrap_or(1)).unwrap_or(0)
}

/// The ready server jobs, in index order.
fn peers<H: Host>(host: &H) -> Vec<NfsDaemonJob> {
    match host.op(NfsOp::Daemons) {
        NfsReply::Daemons { mut jobs } => {
            jobs.retain(|j| j.daemon == Daemon::Svr && j.ready && !j.ending);
            jobs.sort_by_key(|j| j.index);
            jobs
        }
        _ => Vec::new(),
    }
}

/// Whether a v4 client owner (`nfs_client_id4.id`, `co_ownerid`) is of
/// `host`: the host name (or its first label) as a word of it, case
/// ignored. Clients put their own name there (Linux: `Linux NFSv4.1
/// name`; 4.0 adds the server's address); the protocol has no other way
/// to tell a client's host.
#[must_use]
pub fn owner_of_host(owner: &[u8], host: &str) -> bool {
    let o = String::from_utf8_lossy(owner).to_ascii_lowercase();
    let h = host.trim().to_ascii_lowercase();
    if h.is_empty() {
        return false;
    }
    let short = h.split('.').next().unwrap_or(&h).to_string();
    o.split(|c: char| c.is_whitespace() || c == '/' || c == ',' || c == ';')
        .any(|w| w == h || w.split('.').next() == Some(short.as_str()))
}

/// What a job's serving threads share.
struct Shared<H: Host> {
    host: Arc<H>,
    dispatcher: Dispatcher,
    exports: Arc<JobExports>,
    server: Arc<Server>,
    instance: Arc<Instance>,
    /// Connection threads at most (0: no limit), and those running.
    max: usize,
    active: AtomicUsize,
}

impl<H: Host> Shared<H> {
    fn vfs(&self) -> IfsVfs<H::Fs> {
        thread_vfs(&*self.host, &self.instance)
    }
}

/// The IFS's news for the job, on a link of its own: lease breaks of
/// its kept descriptors carried out at once (they yield to a local
/// open they would refuse), the v4 delegations' passed on.
fn news<H: Host>(host: &H, instance: &Arc<Instance>) -> Box<dyn NewsSource> {
    let fs = host.fs().unwrap_or_else(|e| {
        panic!("the NFSv4 server's news thread has no link to the storage service: {e}")
    });
    Box::new(JobNews::new(fs, Arc::clone(instance)))
}

/// Carry out the release orders queued for this job: the host's v4
/// clients lose their state (`NFS4ERR_ADMIN_REVOKED`).
fn orders<H: Host>(
    env: &mut dyn DaemonEnv,
    server: &Server,
    vfs: &OnceCell<IfsVfs<H::Fs>>,
    host: &H,
    instance: &Arc<Instance>,
) {
    let NfsReply::Orders { orders } = env.op(NfsOp::Orders) else {
        return;
    };
    for o in orders {
        let v = vfs.get_or_init(|| thread_vfs(host, instance));
        let n = server.revoke(v, |owner| owner_of_host(owner, &o.host));
        env.log(&Msg::path("CPCA1C9", &o.host), MsgType::Comp);
        env.op(NfsOp::OrderDone {
            id: o.id,
            count: u32::try_from(n).unwrap_or(u32::MAX),
            errno: 0,
        });
    }
}

/// Serve `s` on a thread of `sc`, once the thread limit lets it.
fn serve_here<'s, H: Host>(
    sc: &'s std::thread::Scope<'s, '_>,
    sh: &'s Shared<H>,
    s: TcpStream,
    stop: &'s AtomicBool,
) {
    while sh.max > 0 && sh.active.load(Ordering::Acquire) >= sh.max {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    sh.active.fetch_add(1, Ordering::AcqRel);
    sc.spawn(move || {
        let p = sh.server.program(sh.vfs());
        run::connection(&s, &sh.dispatcher, &p, stop);
        sh.active.fetch_sub(1, Ordering::AcqRel);
    });
}

/// The first job's accept loop: each connection to its job.
fn accept<'s, H: Host>(
    sc: &'s std::thread::Scope<'s, '_>,
    sh: &'s Shared<H>,
    l: &'s Listeners,
    me: u16,
    stop: &'s AtomicBool,
) -> Result<(), Fail> {
    let Some(t) = &l.tcp else { return Ok(()) };
    t.set_nonblocking(true).map_err(|e| Fail::io("TCP", &e))?;
    let jobs: Mutex<(Option<Instant>, Vec<NfsDaemonJob>)> = Mutex::new((None, Vec::new()));
    while !stop.load(Ordering::Relaxed) {
        let s = match t.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(e) => {
                eprintln!("qsjob: QNFSNFSD: accept: {e}");
                std::thread::sleep(TICK);
                continue;
            }
        };
        let _ = s.set_nonblocking(false);
        let target = {
            let mut j = jobs.lock().unwrap_or_else(PoisonError::into_inner);
            if j.0.is_none_or(|t| t.elapsed() >= PEERS_EVERY) {
                *j = (Some(Instant::now()), peers(&*sh.host));
            }
            let ip = s.peer_addr().map(|a| a.ip()).ok();
            match ip {
                Some(ip) if j.1.len() > 1 => j.1.get(pick(ip, j.1.len())).cloned(),
                _ => None,
            }
        };
        match target {
            Some(to) if to.index != me => {
                if let Err(s) = sh.host.give(&to, s) {
                    serve_here(sc, sh, s, stop);
                }
            }
            _ => serve_here(sc, sh, s, stop),
        }
    }
    Ok(())
}

/// Another job's loop: the connections handed to it.
fn taken<'s, H: Host>(sc: &'s std::thread::Scope<'s, '_>, sh: &'s Shared<H>, stop: &'s AtomicBool) {
    while !stop.load(Ordering::Relaxed) {
        if let Some(s) = sh.host.take(500) {
            serve_here(sc, sh, s, stop);
        }
    }
}

/// A job that ends: connections still queued for it go to the first.
fn hand_back<H: Host>(host: &H) {
    let first = peers(host).into_iter().find(|j| j.index == 1);
    while let Some(s) = host.take(1) {
        match &first {
            Some(f) => {
                let _ = host.give(f, s);
            }
            None => drop(s),
        }
    }
}

/// The most threads a server job runs (`QZNFNFSO`), 0 for no limit.
fn thread_limit<H: Host>(host: &H) -> usize {
    match host.op(NfsOp::Options) {
        NfsReply::Options { options } => usize::try_from(options.max_threads).unwrap_or(0),
        _ => 0,
    }
}

/// Serve: the first job binds and registers; every job serves.
#[allow(clippy::too_many_lines)] // one start: sockets, server, loop
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, started: Started, host: &Arc<H>) -> Serve {
    let net = host.net();
    let first = started.index == 1;
    let listeners = if first {
        match running::bind(net, net.nfs_port) {
            Ok(l) => l,
            Err(f) => return Serve::Unavailable(f),
        }
    } else {
        Listeners::default()
    };
    let exports = JobExports::new(host.names());
    match host.fs() {
        Ok(fs) => {
            view::refresh(&**host, &exports, &fs);
        }
        Err(e) => return Serve::Unavailable(Fail::new(e)),
    }
    let regs = running::entries(
        &listeners,
        Prog {
            number: NFS_PROGRAM::NUMBER,
            versions: VERSIONS,
        },
    );
    if first && let Err(f) = running::register(net, &regs) {
        return Serve::Unavailable(f);
    }
    let mut dispatcher = Dispatcher::new();
    dispatcher.set_cache(run::nfs3_cache());
    let instance = Arc::new(Instance::new());
    let table: Arc<dyn ExportTable> = Arc::clone(&exports) as Arc<dyn ExportTable>;
    let server = Server::new(
        Config {
            system: host.system_name().into_bytes(),
            ..Config::default()
        },
        table,
        Arc::clone(&instance),
        Arc::new(SystemClock),
        ids::registry(host),
        Arc::new(IfsStable::new(Arc::clone(host), started.index)),
        Arc::new(TcpConnector::default()),
    );
    let sh = Shared {
        host: Arc::clone(host),
        dispatcher,
        exports,
        server: Arc::clone(&server),
        instance: Arc::clone(&instance),
        max: thread_limit(&**host),
        active: AtomicUsize::new(0),
    };
    let me = started.index;
    let r = Running::spawn("nfs-nfsd", move |stop, ready| {
        let sh = &sh;
        let l = &listeners;
        std::thread::scope(|sc| {
            sc.spawn(|| view::keep_current(&*sh.host, &sh.exports, stop));
            // Leases, grace, recalls; the IFS's lease breaks.
            sc.spawn(|| {
                let v = sh.vfs();
                let mut n = news(&*sh.host, &sh.instance);
                sh.server.background(&v, &mut *n, stop);
            });
            if let Some(u) = &l.udp {
                sc.spawn(move || {
                    let p = sh.server.program(sh.vfs());
                    run::datagrams(u, &sh.dispatcher, &p, stop);
                });
            }
            ready();
            let r = if first {
                accept(sc, sh, l, me, stop)
            } else {
                taken(sc, sh, stop);
                Ok(())
            };
            if !first {
                hand_back(&*sh.host);
            }
            r
        })
    });
    let ports = running::ports(&regs);
    match r {
        Ok(r) => {
            let vfs = OnceCell::new();
            let h = Arc::clone(host);
            let r = r
                .with_ports(ports)
                .with_tick(move |env| orders(env, &server, &vfs, &*h, &instance));
            if first {
                Serve::Ready(r.on_end(move || running::unregister(net, &regs)))
            } else {
                Serve::Ready(r)
            }
        }
        Err(f) => {
            if first {
                running::unregister(net, &regs);
            }
            Serve::Unavailable(f)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_client_one_job() {
        let a: std::net::IpAddr = "10.1.2.3".parse().unwrap();
        let mapped: std::net::IpAddr = "::ffff:10.1.2.3".parse().unwrap();
        assert_eq!(super::pick(a, 3), super::pick(mapped, 3));
        assert_eq!(super::pick(a, 1), 0);
        let spread: std::collections::BTreeSet<usize> = (0..64u8)
            .map(|i| super::pick(std::net::IpAddr::from([10, 0, 0, i]), 4))
            .collect();
        assert_eq!(spread.len(), 4);
    }

    #[test]
    fn a_client_owner_names_its_host() {
        let o = b"Linux NFSv4.1 client1.lab.example";
        assert!(super::owner_of_host(o, "client1"));
        assert!(super::owner_of_host(o, "CLIENT1.lab.example"));
        assert!(super::owner_of_host(
            b"Linux NFSv4.0 client1/10.0.0.5 tcp",
            "client1"
        ));
        assert!(!super::owner_of_host(o, "client"));
        assert!(!super::owner_of_host(o, ""));
    }
}
