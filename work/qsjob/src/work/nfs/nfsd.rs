// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSNFSD` (`NBRSVR` jobs): program 100003. The first job binds TCP
//! and UDP 2049 and registers with the binder; it answers UDP itself
//! and hands each TCP connection to the job its client's address hashes
//! to among the ready server jobs (the descriptor relay, as
//! `QZLSSERVER` hands them to `QZLSFILE`), so one client's state stays
//! in one job. A connection whose job cannot take it, and those left
//! queued for a job that ends, are served by the first job. Each job
//! serves each connection on a thread with a link of its own (all the
//! job's links are one IFS owner) and its own file system; the job's
//! threads share the export table, the duplicate request cache and the
//! start's write verifier. `QZNFNFSO`'s thread limit caps a job's
//! connection threads.

use std::io::{self, Read, Write};
use std::net::{IpAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use nfs::rpc::record::{MAX_RECORD, RecordBuf, frame};
use nfs::rpc::{Dispatcher, LocalProgram, Transport};
use nfs::server::run::Listeners;
use nfs::server::{ExportTable, Gate, Instance, Vfs};
use nfs::types::nfs3::NFS_PROGRAM;
use nfs::v3::Nfs3;
use stg::nfs::{Daemon, NfsDaemonJob, NfsOp, NfsReply};

use super::host::{Host, thread_vfs};
use super::running::{self, Fail, Prog, Running};
use super::view::{self, JobExports};
use super::{DaemonEnv, Serve, Started};

/// How often a waiting thread looks at the stop flag.
const TICK: Duration = Duration::from_millis(100);
/// How often the first job asks which server jobs run.
const PEERS_EVERY: Duration = Duration::from_secs(1);

/// The versions of program 100003 the server registers.
// BLOCKED(rel): NFSv4 in the server job (NFS-6): versions 3 and 4 once `program` builds the combined program.
pub const VERSIONS: &[u32] = &[3];

/// The program one thread serves, over its own file system: the seam
/// where NFS version 4 joins (a program of versions 3 and 4 that calls
/// `v3::call` for 3; docs/NFS.md section 5.9).
pub fn program<V: Vfs>(vfs: V, gate: Gate) -> impl LocalProgram {
    Nfs3::new(vfs, gate)
}

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

/// What a job's serving threads share.
struct Shared<H: Host> {
    host: Arc<H>,
    dispatcher: Dispatcher,
    exports: Arc<JobExports>,
    instance: Arc<Instance>,
    /// Connection threads at most (0: no limit), and those running.
    max: usize,
    active: AtomicUsize,
}

impl<H: Host> Shared<H> {
    fn program(&self) -> impl LocalProgram {
        let table: Arc<dyn ExportTable> = Arc::clone(&self.exports) as Arc<dyn ExportTable>;
        program(thread_vfs(&*self.host, &self.instance), Gate::new(table))
    }
}

/// One TCP connection until it closes or `stop`.
#[allow(clippy::many_single_char_names)] // as nfs::server::run's
fn connection<P: LocalProgram>(s: &TcpStream, d: &Dispatcher, p: &P, stop: &AtomicBool) {
    let Ok(peer) = s.peer_addr() else { return };
    let _ = s.set_read_timeout(Some(TICK));
    let _ = s.set_nodelay(true);
    let mut rb = RecordBuf::new(MAX_RECORD);
    let mut buf = vec![0u8; 64 * 1024];
    let mut w = s;
    while !stop.load(Ordering::Relaxed) {
        match (&mut &*s).read(&mut buf) {
            Ok(0) => return,
            Ok(n) => rb.push(&buf[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(_) => return,
        }
        loop {
            match rb.next_record() {
                Ok(Some(m)) => {
                    if let Some(reply) = d.handle_with(&m, peer, Transport::Tcp, &[p])
                        && w.write_all(&frame(&reply)).is_err()
                    {
                        return;
                    }
                }
                Ok(None) => break,
                Err(_) => return,
            }
        }
        let _ = w.flush();
    }
}

/// UDP until `stop`.
fn datagrams<P: LocalProgram>(u: &UdpSocket, d: &Dispatcher, p: &P, stop: &AtomicBool) {
    let _ = u.set_read_timeout(Some(TICK));
    let mut buf = vec![0u8; 65_536];
    while !stop.load(Ordering::Relaxed) {
        match u.recv_from(&mut buf) {
            Ok((n, peer)) => {
                if let Some(reply) = d.handle_with(&buf[..n], peer, Transport::Udp, &[p]) {
                    let _ = u.send_to(&reply, peer);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => {
                eprintln!("qsjob: QNFSNFSD: UDP: {e}");
                std::thread::sleep(TICK);
            }
        }
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
        let p = sh.program();
        connection(&s, &sh.dispatcher, &p, stop);
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
    dispatcher.set_cache(nfs::server::run::nfs3_cache());
    let sh = Shared {
        host: Arc::clone(host),
        dispatcher,
        exports,
        instance: Arc::new(Instance::new()),
        max: thread_limit(&**host),
        active: AtomicUsize::new(0),
    };
    let me = started.index;
    let r = Running::spawn("nfs-nfsd", move |stop, ready| {
        let sh = &sh;
        let l = &listeners;
        std::thread::scope(|sc| {
            sc.spawn(|| view::keep_current(&*sh.host, &sh.exports, stop));
            if let Some(u) = &l.udp {
                sc.spawn(move || datagrams(u, &sh.dispatcher, &sh.program(), stop));
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
            let r = r.with_ports(ports);
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
}
