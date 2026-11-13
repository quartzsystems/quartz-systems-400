// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The daemons' loops, one per daemon job: [`nfsd`], [`mountd`],
//! [`nlmd`], [`nsmd`]. Each takes its listeners (bound by the job), the
//! binder to register with, a stop flag and what it serves; registers,
//! says it is ready, serves TCP connections on a thread each (each
//! thread making its own program, and so its own [`Vfs`], from the
//! factory) and UDP on one thread, and when `stop` is set stops
//! accepting, lets the threads end, and unregisters. Nothing global is
//! touched: every thread is scoped to the call.

#![allow(clippy::many_single_char_names)]
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::export::ExportTable;
use super::gate::Gate;
use super::news::NewsSource;
use super::vfs::Vfs;
use crate::mount::Mountd;
use crate::mount::list::MountList;
use crate::nlm::Nlm;
use crate::nlm::state::NlmState;
use crate::nsm::Nsm;
use crate::nsm::record::MonRecord;
use crate::nsm::state::NsmState;
use crate::rpc::drc::{self, DuplicateCache};
use crate::rpc::record::{MAX_RECORD, RecordBuf, frame};
use crate::rpc::{ClientConfig, Dispatcher, LocalProgram, Proto, Transport};
use crate::rpcb::BinderClient;
use crate::rpcb::uaddr::to_uaddr;
use crate::types::rpcb::rpcb;

/// How often a waiting thread looks at the stop flag.
const TICK: Duration = Duration::from_millis(100);

/// The sockets a daemon serves, bound by its job.
#[derive(Debug, Default)]
pub struct Listeners {
    /// TCP.
    pub tcp: Option<TcpListener>,
    /// UDP.
    pub udp: Option<UdpSocket>,
}

/// The binder to register with.
#[derive(Debug, Clone)]
pub struct Registration {
    /// Its address (the local host's port 111).
    pub binder: SocketAddr,
    /// The owner of the registrations.
    pub owner: String,
}

/// What every daemon takes.
pub struct Common<'a> {
    /// Its sockets.
    pub listeners: Listeners,
    /// The binder, or none.
    pub binder: Option<Registration>,
    /// Set to stop.
    pub stop: &'a AtomicBool,
    /// Called once registered and serving.
    pub ready: Option<&'a (dyn Fn() + Sync)>,
}

impl std::fmt::Debug for Common<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Common")
            .field("listeners", &self.listeners)
            .field("binder", &self.binder)
            .finish_non_exhaustive()
    }
}

/// The duplicate request cache of NFS version 3's changing procedures.
#[must_use]
pub fn nfs3_cache() -> Arc<DuplicateCache> {
    Arc::new(DuplicateCache::new(drc::TTL, drc::ENTRIES, |p, v, n| {
        p == crate::types::nfs3::NFS_PROGRAM::NUMBER
            && v == 3
            && drc::NFS3_NONIDEMPOTENT.contains(&n)
    }))
}

/// NLM's procedures that are not safe to run twice.
pub const NLM_NONIDEMPOTENT: &[u32] = &[2, 3, 4, 7, 8, 9, 20, 21, 22, 23];

/// The duplicate request cache of NLM's changing procedures.
#[must_use]
pub fn nlm_cache() -> Arc<DuplicateCache> {
    Arc::new(DuplicateCache::new(drc::TTL, drc::ENTRIES, |p, v, n| {
        p == crate::types::nlm::NLM_PROG::NUMBER && v != 2 && NLM_NONIDEMPOTENT.contains(&n)
    }))
}

fn netid(addr: SocketAddr, tcp: bool) -> &'static [u8] {
    match (addr.is_ipv6(), tcp) {
        (false, true) => b"tcp",
        (false, false) => b"udp",
        (true, true) => b"tcp6",
        (true, false) => b"udp6",
    }
}

/// The registrations of `prog` versions `vers` on the listeners.
fn entries(l: &Listeners, prog: u32, vers: &[u32], owner: &str) -> io::Result<Vec<rpcb>> {
    let mut socks = Vec::new();
    if let Some(t) = &l.tcp {
        socks.push((t.local_addr()?, true));
    }
    if let Some(u) = &l.udp {
        socks.push((u.local_addr()?, false));
    }
    let mut out = Vec::new();
    for (addr, tcp) in socks {
        for v in vers {
            out.push(rpcb {
                r_prog: prog,
                r_vers: *v,
                r_netid: netid(addr, tcp).to_vec(),
                r_addr: to_uaddr(addr).into_bytes(),
                r_owner: owner.as_bytes().to_vec(),
            });
        }
    }
    Ok(out)
}

fn binder(reg: &Registration) -> BinderClient {
    BinderClient::at(
        reg.binder,
        Proto::Udp,
        ClientConfig {
            timeout: Duration::from_secs(2),
            retransmits: 2,
            ..ClientConfig::default()
        },
    )
}

fn register(reg: &Registration, list: &[rpcb]) -> io::Result<()> {
    let mut b = binder(reg);
    for r in list {
        // A registration left by an earlier start goes first.
        let _ = b.unset(r);
        match b.set(r) {
            Ok(true) => {}
            Ok(false) => {
                return Err(io::Error::other(format!(
                    "the binder refused program {} version {}",
                    r.r_prog, r.r_vers
                )));
            }
            Err(e) => return Err(io::Error::other(format!("the binder did not answer: {e}"))),
        }
    }
    Ok(())
}

fn unregister(reg: &Registration, list: &[rpcb]) {
    let mut b = binder(reg);
    for r in list {
        let _ = b.unset(r);
    }
}

/// One TCP connection until it closes or `stop` is set.
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
                tracing::warn!("datagram socket failed: {e}");
                std::thread::sleep(TICK);
            }
        }
    }
}

/// Serves `prog` (its `versions` registered) with `d` and a program
/// made by `make` for each thread, until `c.stop`. `extra` runs on a
/// thread of its own beside them (the lock manager's grants).
///
/// # Errors
///
/// A listener's, and a registration the binder refused.
pub fn serve<P, F>(
    c: Common<'_>,
    d: &Dispatcher,
    prog: u32,
    versions: &[u32],
    make: F,
    extra: Option<&(dyn Fn() + Sync)>,
) -> io::Result<()>
where
    P: LocalProgram,
    F: Fn() -> P + Sync,
{
    let Common {
        listeners,
        binder,
        stop,
        ready,
    } = c;
    let c = Parts {
        listeners,
        binder,
        ready,
    };
    let owner = c
        .binder
        .as_ref()
        .map_or_else(String::new, |r| r.owner.clone());
    let list = entries(&c.listeners, prog, versions, &owner)?;
    if let Some(r) = &c.binder {
        register(r, &list)?;
    }
    if let Some(t) = &c.listeners.tcp {
        t.set_nonblocking(true)?;
    }
    let make = &make;
    std::thread::scope(|sc| {
        if let Some(u) = &c.listeners.udp {
            sc.spawn(move || datagrams(u, d, &make(), stop));
        }
        if let Some(x) = extra {
            sc.spawn(x);
        }
        if let Some(f) = c.ready {
            f();
        }
        if let Some(t) = &c.listeners.tcp {
            while !stop.load(Ordering::Relaxed) {
                match t.accept() {
                    Ok((s, _)) => {
                        let _ = s.set_nonblocking(false);
                        sc.spawn(move || connection(&s, d, &make(), stop));
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(TICK),
                    Err(e) => {
                        tracing::warn!("accept failed: {e}");
                        std::thread::sleep(TICK);
                    }
                }
            }
        } else {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(TICK);
            }
        }
    });
    if let Some(r) = &c.binder {
        unregister(r, &list);
    }
    Ok(())
}

/// `QNFSNFSD`: program 100003 as `make` builds it for each thread (an
/// [`crate::v3::Nfs3`], or a program of versions 3 and 4), with the
/// version 3 duplicate request cache.
///
/// # Errors
///
/// Those of [`serve`].
pub fn nfsd<P, F>(c: Common<'_>, make: F) -> io::Result<()>
where
    P: LocalProgram,
    F: Fn() -> P + Sync,
{
    let mut d = Dispatcher::new();
    d.set_cache(nfs3_cache());
    let p = make();
    let (prog, versions) = (p.program(), p.versions().to_vec());
    drop(p);
    serve(c, &d, prog, &versions, make, None)
}

/// `QNFSMNTD`: MOUNT versions 1 and 3 over a [`Vfs`] per thread.
///
/// # Errors
///
/// Those of [`serve`].
pub fn mountd<V, F>(
    c: Common<'_>,
    exports: &Arc<dyn ExportTable>,
    list: &Arc<dyn MountList>,
    vfs: F,
) -> io::Result<()>
where
    V: Vfs,
    F: Fn() -> V + Sync,
{
    let d = Dispatcher::new();
    let make = || Mountd::new(vfs(), Gate::new(Arc::clone(exports)), Arc::clone(list));
    serve(
        c,
        &d,
        crate::types::mount::MOUNT_PROGRAM::NUMBER,
        &[1, 3],
        make,
        None,
    )
}

/// `QNFSNLMD`: NLM versions 1, 3 and 4 (version 2, the monitor's
/// callback, is served but not registered) over a [`Vfs`] per thread;
/// a thread of its own polls `news` for granted locks and calls the
/// clients back, another sends the `_RES` messages.
///
/// # Errors
///
/// Those of [`serve`].
pub fn nlmd<V, F, N>(
    c: Common<'_>,
    exports: &Arc<dyn ExportTable>,
    state: &Arc<NlmState>,
    vfs: F,
    news: N,
) -> io::Result<()>
where
    V: Vfs,
    F: Fn() -> V + Sync,
    N: Fn() -> Box<dyn NewsSource> + Sync,
{
    let mut d = Dispatcher::new();
    d.set_cache(nlm_cache());
    let rx = std::sync::Mutex::new(state.worker());
    let stop = c.stop;
    let st = Arc::clone(state);
    let extra = || {
        std::thread::scope(|sc| {
            sc.spawn(|| {
                let rx = rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                while !stop.load(Ordering::Relaxed) {
                    if let Ok(w) = rx.recv_timeout(TICK) {
                        st.run(w);
                    }
                }
            });
            let v = vfs();
            let mut n = news();
            crate::nlm::grant::grant_loop(&st, &v, &mut *n, stop, 200);
        });
    };
    let make = || Nlm::new(vfs(), Gate::new(Arc::clone(exports)), Arc::clone(state));
    serve(
        c,
        &d,
        crate::types::nlm::NLM_PROG::NUMBER,
        &[1, 3, 4],
        make,
        Some(&extra),
    )
}

/// `QNFSNSMD`: the status monitor; `before` (the hosts monitored before
/// this start, from [`NsmState::start`]) are told it restarted, on a
/// thread of their own, and the callbacks of hosts' notices run on
/// another.
///
/// # Errors
///
/// Those of [`serve`].
pub fn nsmd(c: Common<'_>, state: &Arc<NsmState>, before: Vec<MonRecord>) -> io::Result<()> {
    let d = Dispatcher::new();
    let rx = std::sync::Mutex::new(state.worker());
    let stop = c.stop;
    let st = Arc::clone(state);
    let before = std::sync::Mutex::new(Some(before));
    let extra = || {
        std::thread::scope(|sc| {
            sc.spawn(|| {
                let hosts = before
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .unwrap_or_default();
                let left = st.notify_hosts(&hosts, stop);
                for h in left {
                    tracing::warn!(
                        "host {} not told of the restart",
                        String::from_utf8_lossy(&h)
                    );
                }
            });
            let rx = rx.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            while !stop.load(Ordering::Relaxed) {
                if let Ok(w) = rx.recv_timeout(TICK) {
                    st.run(w, stop);
                }
            }
        });
    };
    let nsm = Nsm::new(Arc::clone(state));
    serve(
        c,
        &d,
        crate::types::nsm::SM_PROG::NUMBER,
        &[1],
        || nsm.clone(),
        Some(&extra),
    )
}

/// [`Common`] taken apart by [`serve`].
struct Parts<'a> {
    listeners: Listeners,
    binder: Option<Registration>,
    ready: Option<&'a (dyn Fn() + Sync)>,
}
