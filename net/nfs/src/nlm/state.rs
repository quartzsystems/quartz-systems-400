// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the lock manager's threads share: the lock owners as IFS
//! sub-owners, the hosts holding locks (and whether the status monitor
//! watches them), the blocked requests by their wait token, the share
//! reservations, the grace period, and the work to do after a reply
//! (callbacks to clients).
//!
//! A lock owner is (caller name, `svid`, owner handle); each gets a
//! sub-owner number from [`SUB_BASE`] up, so the IFS sees every
//! client's owner as its own holder, conflicting with the others and
//! with local jobs. Host names are compared without regard to case.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

use crate::rpc::ClientConfig;
use crate::server::locate::Locator;
use crate::server::vfs::{LockKind, Req, Vfs};
use crate::server::{Clock, FileHandle};

/// The first sub-owner number the lock manager gives (`NL` in the top
/// bytes, apart from any other server's numbers).
pub const SUB_BASE: u64 = 0x4E4C_0000_0000_0000;

/// The grace period by default.
pub const GRACE: Duration = Duration::from_secs(90);

/// A lock as the protocol names it, 64-bit (versions 1 and 3 widened).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Lock {
    /// The client's name for itself.
    pub caller_name: Vec<u8>,
    /// The file's handle.
    pub fh: Vec<u8>,
    /// The owner handle.
    pub oh: Vec<u8>,
    /// The client's process.
    pub svid: i32,
    /// The first byte.
    pub offset: u64,
    /// The length, 0 to the end.
    pub len: u64,
}

/// An owner: (host, `svid`, owner handle).
pub type OwnerKey = (Vec<u8>, i32, Vec<u8>);

/// A lock waiting in the IFS.
#[derive(Debug, Clone)]
pub struct Blocked {
    /// The sub-owner.
    pub sub: u64,
    /// The lock asked for.
    pub lock: Lock,
    /// Exclusive.
    pub exclusive: bool,
    /// The protocol version asked in.
    pub vers: u32,
    /// Whether it came as `NLM_LOCK_MSG` (the grant goes as `GRANTED_MSG`).
    pub msg: bool,
    /// The client.
    pub peer: SocketAddr,
    /// The request, for undoing the lock when the grant is not taken.
    pub req: Req,
    /// The handle.
    pub handle: FileHandle,
    /// The client's cookie.
    pub cookie: Vec<u8>,
}

/// A share reservation held.
#[derive(Debug, Clone, Copy)]
pub struct Share {
    /// The open's number.
    pub open: u64,
}

/// A grant sent as `GRANTED_MSG`, until its `GRANTED_RES`.
#[derive(Debug, Clone)]
pub struct Granted {
    /// The lock.
    pub blocked: Blocked,
    /// When it was sent.
    pub at: Instant,
}

/// What a lock manager thread does after it answered.
#[derive(Debug, Clone)]
pub enum Work {
    /// Send an `_RES` (procedure, version, encoded results) to a client.
    Res(SocketAddr, u32, u32, Vec<u8>),
}

/// How the lock manager works.
#[derive(Clone)]
pub struct NlmConfig {
    /// Watch each locking host through the local status monitor.
    pub monitor: bool,
    /// Where programs listen (clients' lock managers, the local monitor).
    pub locator: Arc<dyn Locator>,
    /// How callbacks are made.
    pub rpc: ClientConfig,
    /// The grace period after a start.
    pub grace: Duration,
    /// Tries of a `GRANTED` callback before the lock is given back.
    pub grant_tries: u32,
    /// The clock of the grace period.
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for NlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NlmConfig")
            .field("monitor", &self.monitor)
            .field("grace", &self.grace)
            .finish_non_exhaustive()
    }
}

/// A share reservation's holder: (host, owner handle, file handle).
type ShareKey = (Vec<u8>, Vec<u8>, Vec<u8>);

#[derive(Debug, Default)]
struct Tables {
    owners: HashMap<OwnerKey, u64>,
    by_sub: HashMap<u64, OwnerKey>,
    next: u64,
    /// Hosts holding locks, and whether they are monitored.
    hosts: HashMap<Vec<u8>, bool>,
    blocked: HashMap<u64, Blocked>,
    shares: HashMap<ShareKey, Share>,
    granted: HashMap<Vec<u8>, Granted>,
    cookie: u64,
    /// Tokens granted before their request was kept.
    early: Vec<u64>,
    /// Requests granted, their callbacks not yet sent.
    ready: Vec<Blocked>,
}

/// The lock manager's shared state.
pub struct NlmState {
    /// How it works.
    pub cfg: NlmConfig,
    started: Instant,
    t: Mutex<Tables>,
    work: Mutex<Option<mpsc::SyncSender<Work>>>,
    client: Mutex<Option<Arc<dyn LockClient>>>,
}

impl std::fmt::Debug for NlmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NlmState")
            .field("cfg", &self.cfg)
            .finish_non_exhaustive()
    }
}

/// A host name as kept.
pub(crate) fn host_key(n: &[u8]) -> Vec<u8> {
    n.to_ascii_lowercase()
}

impl NlmState {
    /// A start: its grace period begins now.
    pub fn new(cfg: NlmConfig) -> Arc<Self> {
        let started = cfg.clock.now();
        Arc::new(Self {
            cfg,
            started,
            t: Mutex::new(Tables {
                next: SUB_BASE,
                ..Tables::default()
            }),
            work: Mutex::new(None),
            client: Mutex::new(None),
        })
    }

    fn t(&self) -> MutexGuard<'_, Tables> {
        self.t
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether only reclaims are taken.
    pub fn in_grace(&self) -> bool {
        self.cfg.clock.now() < self.started + self.cfg.grace
    }

    /// The sub-owner of a lock's owner (made on first use).
    pub fn sub_of(&self, l: &Lock) -> u64 {
        let key = (host_key(&l.caller_name), l.svid, l.oh.clone());
        let mut t = self.t();
        if let Some(s) = t.owners.get(&key) {
            return *s;
        }
        t.next += 1;
        let s = t.next;
        t.owners.insert(key.clone(), s);
        t.by_sub.insert(s, key);
        s
    }

    /// The owner of a sub-owner of ours: (`svid`, owner handle).
    pub fn owner_of(&self, sub: u64) -> Option<(i32, Vec<u8>)> {
        self.t().by_sub.get(&sub).map(|k| (k.1, k.2.clone()))
    }

    /// Whether the host needs monitoring (not yet watched); marks it known.
    pub fn needs_monitor(&self, host: &[u8]) -> bool {
        let mut t = self.t();
        let e = t.hosts.entry(host_key(host)).or_insert(false);
        !*e
    }

    /// The host is watched now.
    pub fn monitored(&self, host: &[u8]) {
        self.t().hosts.insert(host_key(host), true);
    }

    /// Keeps a blocked request under its token. One whose grant came
    /// first is ready for its callback at once.
    pub fn block(&self, token: u64, b: Blocked) {
        let mut t = self.t();
        if let Some(i) = t.early.iter().position(|x| *x == token) {
            t.early.swap_remove(i);
            t.ready.push(b);
        } else {
            t.blocked.insert(token, b);
        }
    }

    /// A token the IFS granted: its request, now ready for its callback.
    /// One not kept yet is remembered (its request is being kept).
    pub fn granted_token(&self, token: u64) {
        let mut t = self.t();
        if let Some(b) = t.blocked.remove(&token) {
            t.ready.push(b);
        } else {
            if t.early.len() >= 4096 {
                t.early.remove(0);
            }
            t.early.push(token);
        }
    }

    /// The requests whose callbacks are to be sent.
    pub fn take_ready(&self) -> Vec<Blocked> {
        std::mem::take(&mut self.t().ready)
    }

    /// The lock client's side, if this host has one.
    pub fn set_client(&self, c: Arc<dyn LockClient>) {
        *self
            .client
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(c);
    }

    /// The lock client.
    pub fn client(&self) -> Option<Arc<dyn LockClient>> {
        self.client
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The token of a waiting request for the same owner, file, range and kind.
    pub fn waiting(&self, sub: u64, l: &Lock, exclusive: bool) -> Option<u64> {
        self.t()
            .blocked
            .iter()
            .find(|(_, b)| {
                b.sub == sub
                    && b.lock.fh == l.fh
                    && b.lock.offset == l.offset
                    && b.lock.len == l.len
                    && b.exclusive == exclusive
            })
            .map(|(t, _)| *t)
    }

    /// Takes the blocked request of a token.
    pub fn unblock(&self, token: u64) -> Option<Blocked> {
        self.t().blocked.remove(&token)
    }

    /// Blocked requests now.
    pub fn blocked_count(&self) -> usize {
        self.t().blocked.len()
    }

    /// A new cookie for a callback.
    pub fn cookie(&self) -> Vec<u8> {
        let mut t = self.t();
        t.cookie += 1;
        t.cookie.to_be_bytes().to_vec()
    }

    /// Remembers a grant sent as a message.
    pub fn sent_grant(&self, cookie: Vec<u8>, b: Blocked) {
        let at = self.cfg.clock.now();
        self.t().granted.insert(cookie, Granted { blocked: b, at });
    }

    /// The grant a `GRANTED_RES` answers.
    pub fn grant_answered(&self, cookie: &[u8]) -> Option<Granted> {
        self.t().granted.remove(cookie)
    }

    /// The share reservation of (host, owner handle, file).
    pub fn share(&self, host: &[u8], oh: &[u8], fh: &[u8]) -> Option<Share> {
        self.t()
            .shares
            .get(&(host_key(host), oh.to_vec(), fh.to_vec()))
            .copied()
    }

    /// Keeps a share reservation: the one it replaces.
    pub fn put_share(&self, host: &[u8], oh: &[u8], fh: &[u8], s: Share) -> Option<Share> {
        self.t()
            .shares
            .insert((host_key(host), oh.to_vec(), fh.to_vec()), s)
    }

    /// Forgets a share reservation.
    pub fn take_share(&self, host: &[u8], oh: &[u8], fh: &[u8]) -> Option<Share> {
        self.t()
            .shares
            .remove(&(host_key(host), oh.to_vec(), fh.to_vec()))
    }

    /// Everything `host` holds goes: its locks and waiting requests (the
    /// IFS's, through `vfs`), its shares. How many locks went.
    /// `FREE_ALL`, a host's restart told by the status monitor, and
    /// `RLSIFSLCK RMTLOCNAME`.
    pub fn release_client(&self, vfs: &dyn Vfs, host: &[u8]) -> u32 {
        let h = host_key(host);
        let (subs, shares) = {
            let mut t = self.t();
            let subs: Vec<u64> = t
                .owners
                .iter()
                .filter(|(k, _)| k.0 == h)
                .map(|(_, s)| *s)
                .collect();
            t.owners.retain(|k, _| k.0 != h);
            for s in &subs {
                t.by_sub.remove(s);
            }
            t.blocked.retain(|_, b| !subs.contains(&b.sub));
            t.ready.retain(|b| !subs.contains(&b.sub));
            t.granted.retain(|_, g| !subs.contains(&g.blocked.sub));
            let shares: Vec<Share> = t
                .shares
                .iter()
                .filter(|(k, _)| k.0 == h)
                .map(|(_, s)| *s)
                .collect();
            t.shares.retain(|k, _| k.0 != h);
            t.hosts.remove(&h);
            (subs, shares)
        };
        let mut n = 0;
        for s in subs {
            n += vfs.release_sub(s).unwrap_or(0);
        }
        for s in shares {
            let _ = vfs.close(s.open);
        }
        n
    }

    /// The hosts holding locks or shares now.
    pub fn hosts(&self) -> Vec<Vec<u8>> {
        let t = self.t();
        let mut v: Vec<Vec<u8>> = t.owners.keys().map(|k| k.0.clone()).collect();
        v.extend(t.shares.keys().map(|k| k.0.clone()));
        v.sort();
        v.dedup();
        v
    }

    /// From now on [`Work`] goes to a worker: what it reads.
    pub fn worker(&self) -> mpsc::Receiver<Work> {
        let (tx, rx) = mpsc::sync_channel(256);
        *self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);
        rx
    }

    /// Does `w` on the worker, or now when there is none.
    pub fn later(&self, w: Work) {
        let tx = self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        match tx {
            Some(tx) => {
                if tx.try_send(w).is_err() {
                    tracing::warn!("lock manager work dropped: the queue is full");
                }
            }
            None => self.run(w),
        }
    }

    /// Does `w`.
    pub fn run(&self, w: Work) {
        match w {
            Work::Res(peer, vers, procedure, results) => {
                super::call::send_res(self, peer, vers, procedure, &results);
            }
        }
    }
}

/// The kind of an exclusive flag.
#[must_use]
pub fn kind(exclusive: bool) -> LockKind {
    if exclusive {
        LockKind::Write
    } else {
        LockKind::Read
    }
}

/// The lock client's side of the protocol, when this host also mounts
/// with locks: what a server sends back to it (`NLM_GRANTED`, the
/// `_RES` messages but `GRANTED_RES`). Without one, `NLM_GRANTED` is
/// refused (`DENIED`: no request of ours waits) and the `_RES` messages
/// are `PROC_UNAVAIL`.
pub trait LockClient: Send + Sync {
    /// A server says a lock this host waited for is held: whether it is
    /// still wanted.
    fn granted(&self, peer: SocketAddr, exclusive: bool, lock: &Lock) -> bool;
    /// A server's answer to an `_MSG` call: the procedure (11 to 14)
    /// and its results, as version 4's.
    fn result(&self, peer: SocketAddr, procedure: u32, res: &Reply);
}

/// A server's answer an `_RES` message carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// `TEST_RES`.
    Test(crate::types::nlm::nlm4_testres),
    /// `LOCK_RES`, `CANCEL_RES`, `UNLOCK_RES`.
    Res(crate::types::nlm::nlm4_res),
}
