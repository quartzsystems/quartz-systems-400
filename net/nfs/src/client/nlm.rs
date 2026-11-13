// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte-range locks of a version 3 mount through the server's lock
//! manager (NLM version 4). Requests never block (`EAGAIN` on a
//! conflict). The first lock asks this system's status monitor to watch
//! the server (`SM_MON`, calling back this system's lock manager with
//! `NLM_SM_NOTIFY` and [`PRIV_TAG`] in the private bytes); without the
//! local monitor locks are `ENOLCK`. The locks held are remembered and
//! taken again with `reclaim` when the server restarted
//! ([`super::restart`]).

// Channel, count, cookie: the short names are the protocol's.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::remote::{LockConflict, LockKind, LockOwner, RResult, RemoteError};

use super::channel::{Channel, Status};
use super::env::ClientEnv;
use super::ranges::{self, Range};
use super::settings::Policy;
use crate::rpc::{ClientConfig, Credential, Proto};
use crate::rpcb::BinderClient;
use crate::server::locate::Host;
use crate::types::nlm::NLM_PROG::NLM4_VERS::{Args, Res};
use crate::types::nlm::{
    NLM_PROG, netobj, nlm4_lock, nlm4_lockargs, nlm4_stats, nlm4_testargs, nlm4_testrply,
    nlm4_unlockargs,
};
use crate::types::nsm::{SM_PROG, mon, mon_id, my_id, res};

/// The private bytes of this client's `SM_MON`: a notice carrying them
/// is about a server this system mounts.
pub const PRIV_TAG: [u8; 16] = *b"QSNFSCLI\0\0\0\0\0\0\0\0";

/// `NLM_SM`, the version the status monitor calls back.
const NLM_SM_VERS: i32 = 2;
/// `NLM_SM_NOTIFY`.
const NLM_SM_NOTIFY: i32 = 1;

/// The ranges each (file handle, owner) holds on the server.
type Held = HashMap<(Vec<u8>, u64), Vec<Range>>;

/// The lock client of one mount.
pub struct Locker {
    env: ClientEnv,
    server: IpAddr,
    /// The server as `MFS` names it: what the status monitor watches.
    name: String,
    proto: Proto,
    policy: Policy,
    status: Arc<Status>,
    nlm: Mutex<Option<Arc<Channel>>>,
    monitored: AtomicBool,
    nsm_state: AtomicI32,
    cookie: AtomicU64,
    held: Mutex<Held>,
}

impl std::fmt::Debug for Locker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Locker")
            .field("server", &self.server)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// How often a reclaim looks for a restarted server's lock manager.
const RECLAIM_AGAIN: Duration = Duration::from_millis(250);

fn nolck() -> RemoteError {
    RemoteError::Errno(Errno::ENOLCK)
}

impl Locker {
    /// The lock client of a mount of `name` (at `server`).
    #[must_use]
    pub fn new(
        env: ClientEnv,
        server: IpAddr,
        name: &str,
        proto: Proto,
        policy: Policy,
        status: Arc<Status>,
    ) -> Arc<Self> {
        let l = Arc::new(Self {
            env,
            server,
            name: name.to_string(),
            proto,
            policy,
            status,
            nlm: Mutex::new(None),
            monitored: AtomicBool::new(false),
            nsm_state: AtomicI32::new(0),
            cookie: AtomicU64::new(1),
            held: Mutex::new(HashMap::new()),
        });
        super::restart::register(&l);
        l
    }

    /// Whether `name` (a status monitor's `mon_name`) is this mount's
    /// server.
    #[must_use]
    pub fn is_server(&self, name: &[u8]) -> bool {
        let n = String::from_utf8_lossy(name);
        let n = n.trim();
        n.eq_ignore_ascii_case(&self.name) || n.parse::<IpAddr>().is_ok_and(|ip| ip == self.server)
    }

    fn rpc_cfg(&self) -> ClientConfig {
        ClientConfig {
            timeout: self.policy.timeo,
            retransmits: self.policy.retrans,
            reserved_port: false,
            ..ClientConfig::default()
        }
    }

    /// The server's lock manager, found through its binder once.
    fn channel(&self) -> RResult<Arc<Channel>> {
        let mut g = self.nlm.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(c) = &*g {
            return Ok(Arc::clone(c));
        }
        let protocol = match self.proto {
            Proto::Tcp => 6,
            Proto::Udp => 17,
        };
        let mut b = BinderClient::at(self.env.binder(self.server), Proto::Udp, self.rpc_cfg());
        let port = b
            .getport(NLM_PROG::NUMBER, 4, protocol)
            .ok()
            .flatten()
            .ok_or_else(nolck)?;
        let c = Arc::new(Channel::new(
            SocketAddr::new(self.server, port),
            self.proto,
            self.policy,
            Arc::clone(&self.status),
            self.env.reserved_port,
            self.env.connections,
        ));
        *g = Some(Arc::clone(&c));
        Ok(c)
    }

    /// Ask this system's status monitor to watch the server (once).
    fn monitor(&self) -> RResult<()> {
        if self.monitored.load(Ordering::Relaxed) {
            return Ok(());
        }
        let local = self.env.local.as_ref().ok_or_else(nolck)?;
        let at = local
            .find(
                &Host::Addr(IpAddr::from([127, 0, 0, 1])),
                SM_PROG::NUMBER,
                1,
                Proto::Udp,
            )
            .ok_or_else(nolck)?;
        let m = mon {
            mon_id: mon_id {
                mon_name: self.name.as_bytes().to_vec(),
                my_id: my_id {
                    my_name: b"127.0.0.1".to_vec(),
                    my_prog: NLM_PROG::NUMBER.cast_signed(),
                    my_vers: NLM_SM_VERS,
                    my_proc: NLM_SM_NOTIFY,
                },
            },
            r#priv: PRIV_TAG,
        };
        match crate::nsm::call::sm_mon(at, &self.rpc_cfg(), &m) {
            Ok(r) if r.res_stat == res::stat_succ => {
                self.nsm_state.store(r.state, Ordering::Relaxed);
                self.monitored.store(true, Ordering::Relaxed);
                Ok(())
            }
            Ok(_) | Err(_) => Err(nolck()),
        }
    }

    fn cookie(&self) -> netobj {
        netobj(
            self.cookie
                .fetch_add(1, Ordering::Relaxed)
                .to_be_bytes()
                .to_vec(),
        )
    }

    fn alock(&self, fh: &[u8], owner: LockOwner, off: u64, len: u64) -> nlm4_lock {
        nlm4_lock {
            caller_name: self.env.host_name.as_bytes().to_vec(),
            fh: netobj(fh.to_vec()),
            oh: netobj(owner.0.to_be_bytes().to_vec()),
            svid: i32::try_from(owner.0 & 0x7fff_ffff).unwrap_or(0),
            l_offset: off,
            l_len: len,
        }
    }

    fn cred(&self) -> Credential {
        Credential::sys(self.env.host_name.as_bytes(), 0, 0, &[])
    }

    /// A status other than granted as an error; `None` for the grace
    /// period (wait and ask again).
    fn refused(s: nlm4_stats) -> Option<RemoteError> {
        Some(match s {
            nlm4_stats::NLM4_DENIED_GRACE_PERIOD => return None,
            nlm4_stats::NLM4_DENIED | nlm4_stats::NLM4_BLOCKED => RemoteError::Errno(Errno::EAGAIN),
            nlm4_stats::NLM4_DENIED_NOLOCKS => nolck(),
            nlm4_stats::NLM4_DEADLCK => RemoteError::Errno(Errno::EDEADLK),
            nlm4_stats::NLM4_ROFS => RemoteError::Errno(Errno::EROFS),
            nlm4_stats::NLM4_STALE_FH => RemoteError::Stale,
            nlm4_stats::NLM4_FBIG => RemoteError::Errno(Errno::EFBIG),
            nlm4_stats::NLM4_GRANTED | nlm4_stats::NLM4_FAILED => RemoteError::Errno(Errno::EIO),
        })
    }

    /// Waits in the server's grace period: a soft mount gives up after
    /// one major timeout (`ENOLCK`).
    fn grace_wait(&self, n: &mut u32) -> RResult<()> {
        *n += 1;
        let wait = self
            .policy
            .wait((*n).min(5))
            .max(Duration::from_millis(100));
        if self.policy.soft && wait.saturating_mul(*n) > self.policy.major() {
            return Err(nolck());
        }
        std::thread::sleep(wait.min(Duration::from_secs(5)));
        Ok(())
    }

    fn send_lock(
        &self,
        fh: &[u8],
        owner: LockOwner,
        exclusive: bool,
        off: u64,
        len: u64,
        reclaim: bool,
    ) -> RResult<()> {
        let c = self.channel()?;
        let mut n = 0;
        loop {
            let a = nlm4_lockargs {
                cookie: self.cookie(),
                block: false,
                exclusive,
                alock: self.alock(fh, owner, off, len),
                reclaim,
                state: self.nsm_state.load(Ordering::Relaxed),
            };
            let Res::NLMPROC4_LOCK(r) =
                c.call::<Args, Res>(&self.cred(), &Args::NLMPROC4_LOCK(a))?
            else {
                return Err(RemoteError::Errno(Errno::EIO));
            };
            if r.stat.stat == nlm4_stats::NLM4_GRANTED {
                return Ok(());
            }
            match Self::refused(r.stat.stat) {
                Some(e) => return Err(e),
                None => self.grace_wait(&mut n)?,
            }
        }
    }

    /// Take a lock without waiting.
    ///
    /// # Errors
    ///
    /// `EAGAIN` on a conflict, `ENOLCK` without the status monitors.
    pub fn lock(
        &self,
        fh: &[u8],
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        self.monitor()?;
        let exclusive = kind == LockKind::Write;
        self.send_lock(fh, owner, exclusive, off, len, false)?;
        let mut h = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        ranges::add(
            h.entry((fh.to_vec(), owner.0)).or_default(),
            Range {
                exclusive,
                off,
                len,
            },
        );
        Ok(())
    }

    /// Release a range.
    ///
    /// # Errors
    ///
    /// The server's.
    pub fn unlock(&self, fh: &[u8], owner: LockOwner, off: u64, len: u64) -> RResult<()> {
        let c = self.channel()?;
        let mut n = 0;
        loop {
            let a = nlm4_unlockargs {
                cookie: self.cookie(),
                alock: self.alock(fh, owner, off, len),
            };
            let Res::NLMPROC4_UNLOCK(r) =
                c.call::<Args, Res>(&self.cred(), &Args::NLMPROC4_UNLOCK(a))?
            else {
                return Err(RemoteError::Errno(Errno::EIO));
            };
            match r.stat.stat {
                nlm4_stats::NLM4_GRANTED | nlm4_stats::NLM4_DENIED => break,
                s => match Self::refused(s) {
                    Some(e) => return Err(e),
                    None => self.grace_wait(&mut n)?,
                },
            }
        }
        let mut h = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(v) = h.get_mut(&(fh.to_vec(), owner.0)) {
            ranges::cut(v, off, len);
            if v.is_empty() {
                h.remove(&(fh.to_vec(), owner.0));
            }
        }
        Ok(())
    }

    /// The lock that would stand in the way.
    ///
    /// # Errors
    ///
    /// The server's.
    pub fn test(
        &self,
        fh: &[u8],
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        let c = self.channel()?;
        let mut n = 0;
        loop {
            let a = nlm4_testargs {
                cookie: self.cookie(),
                exclusive: kind == LockKind::Write,
                alock: self.alock(fh, owner, off, len),
            };
            let Res::NLMPROC4_TEST(r) =
                c.call::<Args, Res>(&self.cred(), &Args::NLMPROC4_TEST(a))?
            else {
                return Err(RemoteError::Errno(Errno::EIO));
            };
            match r.test_stat {
                nlm4_testrply::NLM4_DENIED(h) => {
                    return Ok(Some(LockConflict {
                        kind: if h.exclusive {
                            LockKind::Write
                        } else {
                            LockKind::Read
                        },
                        off: h.l_offset,
                        len: h.l_len,
                        holder: u64::from(h.svid.cast_unsigned()),
                    }));
                }
                nlm4_testrply::default(nlm4_stats::NLM4_GRANTED) => return Ok(None),
                nlm4_testrply::default(s) => match Self::refused(s) {
                    Some(e) => return Err(e),
                    None => self.grace_wait(&mut n)?,
                },
            }
        }
    }

    /// The server restarted (its monitor's state is now `state`): take
    /// every held lock again with `reclaim`. How many came back.
    pub fn reclaim(&self) -> usize {
        // The server's lock manager may have a new port.
        *self.nlm.lock().unwrap_or_else(PoisonError::into_inner) = None;
        let held = self
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut got = 0;
        // The server's status monitor starts before its lock manager
        // (`STRNFSSVR` starts `*NSM` first) and tells this host at once:
        // its lock manager is looked for again until the grace period
        // the reclaim must land in is over.
        let until = Instant::now() + crate::nlm::state::GRACE;
        let send = |fh: &[u8], owner: u64, r: &Range| loop {
            match self.send_lock(fh, LockOwner(owner), r.exclusive, r.off, r.len, true) {
                Err(e) if e == nolck() && Instant::now() < until => {
                    *self.nlm.lock().unwrap_or_else(PoisonError::into_inner) = None;
                    std::thread::sleep(RECLAIM_AGAIN);
                }
                other => break other,
            }
        };
        for ((fh, owner), rs) in &held {
            for r in rs {
                match send(fh, *owner, r) {
                    Ok(()) => got += 1,
                    Err(e) => {
                        tracing::warn!(?e, server = %self.name, "a lock was not reclaimed");
                        let mut v = self.held.lock().unwrap_or_else(PoisonError::into_inner);
                        if let Some(l) = v.get_mut(&(fh.clone(), *owner)) {
                            ranges::cut(l, r.off, r.len);
                        }
                    }
                }
            }
        }
        got
    }

    /// Locks held now.
    #[must_use]
    pub fn held(&self) -> usize {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(Vec::len)
            .sum()
    }
}
