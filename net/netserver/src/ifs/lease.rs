// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's leases in the IFS's lease table (`ifs::lease`), so other
//! jobs' changes break them (`docs/NETSERVER.md` section 6):
//!
//! - before the library grants a lease, [`LeaseLink::limit`] reserves it
//!   in the table, capped by what other jobs hold and have open;
//! - every change to a file's leases the library makes ([`Observer`])
//!   goes to the table as the file's full set, in order with the job's
//!   other IFS calls (the [`IfsWorker`]); a lease still being granted is
//!   reported as `KEEP`;
//! - a thread of its own polls the table for break orders on a second
//!   client and has the library break the file's leases
//!   ([`LeaseLink::serve_orders`]); the library's report then completes
//!   the break.
//! - the same poll brings the changes of the directories the job's
//!   clients watch ([`super::watch`]).
//!
//! Until [`LeaseLink::start`] the link is off and leases are the job's
//! own, as without it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ifs::client::IfsClient;
use ifs::lease::LeaseKey as Key;
use ifs::lease::state::{KEEP, RWH};
use ifs::ops::audit::FileId;
use smb_server::{LeaseKey, LeaseObserver};

use super::info::file_index;
use super::watch::Watches;
use super::worker::IfsWorker;

/// How long one poll for orders waits, in milliseconds.
const POLL_MS: u32 = 30_000;

/// Off: leases are the job's own. On: in the table. Gone: the poll
/// failed, so no new lease is granted.
const OFF: u8 = 0;
const ON: u8 = 1;
const GONE: u8 = 2;

#[derive(Default)]
struct Inner {
    /// The library's last report per file, per share.
    reported: HashMap<u64, HashMap<String, Vec<(Key, u8)>>>,
    /// Leases being granted, per file: reserved, not yet reported.
    pending: HashMap<u64, Vec<Key>>,
}

/// The job's side of the lease table.
pub struct LeaseLink {
    io: Arc<IfsWorker>,
    state: AtomicU8,
    inner: Mutex<Inner>,
    watches: Arc<Watches>,
}

impl LeaseLink {
    /// A link over the job's IFS thread, off.
    #[must_use]
    pub fn new(io: Arc<IfsWorker>) -> Arc<Self> {
        Arc::new(Self {
            io,
            state: AtomicU8::new(OFF),
            inner: Mutex::default(),
            watches: Arc::default(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether leases go to the table.
    #[must_use]
    pub fn is_on(&self) -> bool {
        self.state.load(Ordering::SeqCst) == ON
    }

    /// The job's directory watches, fed by the same poll.
    #[must_use]
    pub fn watches(&self) -> &Arc<Watches> {
        &self.watches
    }

    /// Turn the link on: leases from now on go to the table.
    pub fn start(&self) {
        self.state.store(ON, Ordering::SeqCst);
    }

    /// The poll for orders failed: no lease is granted from now on.
    pub fn gone(&self) {
        self.state.store(GONE, Ordering::SeqCst);
    }

    /// The most caching the lease `lease` may have on the file `id` at
    /// `path`, reserved in the table (`reserved` keeps its key until the
    /// open closes); `want` while the link is off.
    pub async fn limit(
        self: &Arc<Self>,
        id: Option<FileId>,
        path: String,
        lease: LeaseKey,
        want: u32,
        reserved: &Mutex<Vec<Key>>,
    ) -> u32 {
        match self.state.load(Ordering::SeqCst) {
            OFF => return want,
            GONE => return 0,
            _ => {}
        }
        let Some(id) = id else { return 0 };
        let tag = file_index(id.fs, id.ino, id.generation);
        let key = lease.to_bytes();
        self.lock().pending.entry(tag).or_default().push(key);
        reserved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(key);
        let want = u8::try_from(want & u32::from(RWH)).unwrap_or(0);
        let granted = self
            .io
            .call(move |c| {
                c.lease_grant(
                    &super::handle::locate(c, Some(id), &path),
                    id,
                    tag,
                    key,
                    want,
                )
            })
            .await;
        u32::from(granted.unwrap_or(0))
    }

    /// The open of the file `id` closed: the leases it reserved that the
    /// library never granted (`keys`) go.
    pub fn closed(&self, id: Option<FileId>, keys: &[Key]) {
        if !self.is_on() {
            return;
        }
        let Some(id) = id else { return };
        let tag = file_index(id.fs, id.ino, id.generation);
        let mut g = self.lock();
        let Some(p) = g.pending.get_mut(&tag) else {
            return;
        };
        let before = p.len();
        p.retain(|k| !keys.contains(k));
        let dropped = p.len() != before;
        if p.is_empty() {
            g.pending.remove(&tag);
        }
        if dropped {
            self.send(&g, tag);
        }
    }

    /// The library changed the leases of `tag` on `share`.
    fn changed(&self, share: &str, tag: u64, leases: &[(LeaseKey, u32)]) {
        if !self.is_on() {
            return;
        }
        let mut g = self.lock();
        let now: Vec<(Key, u8)> = leases
            .iter()
            .map(|(k, s)| (k.to_bytes(), u8::try_from(s & u32::from(RWH)).unwrap_or(0)))
            .collect();
        if let Some(p) = g.pending.get_mut(&tag) {
            p.retain(|k| !now.iter().any(|(n, _)| n == k));
            if p.is_empty() {
                g.pending.remove(&tag);
            }
        }
        let per_share = g.reported.entry(tag).or_default();
        if now.is_empty() {
            per_share.remove(share);
        } else {
            per_share.insert(share.to_owned(), now);
        }
        if per_share.is_empty() {
            g.reported.remove(&tag);
        }
        self.send(&g, tag);
    }

    /// Report what the job holds on `tag`: every share's leases and the
    /// ones being granted, queued behind the job's other IFS calls.
    fn send(&self, g: &Inner, tag: u64) {
        let mut all: Vec<(Key, u8)> = g
            .reported
            .get(&tag)
            .map(|m| m.values().flatten().copied().collect())
            .unwrap_or_default();
        for k in g.pending.get(&tag).into_iter().flatten() {
            all.push((*k, KEEP));
        }
        self.io.post(move |c| {
            let _ = c.lease_report(tag, &all);
        });
    }

    /// Serve break orders until the client fails or `stop` is set: `spawn`
    /// has the library break a file's leases to the caching it may keep.
    pub fn serve_orders(
        self: &Arc<Self>,
        client: &dyn IfsClient,
        stop: &AtomicBool,
        mut spawn: impl FnMut(u64, u32, Arc<Self>),
    ) {
        while !stop.load(Ordering::SeqCst) {
            match client.lease_orders(POLL_MS) {
                Ok(news) => {
                    for (id, events) in news.changes {
                        self.watches.deliver(id, events);
                    }
                    for (tag, keep) in news.breaks {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        spawn(tag, u32::from(keep), self.clone());
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "lease orders: the poll failed; no more leases");
                    self.gone();
                    return;
                }
            }
        }
    }

    /// The server ends: the job's leases and watches leave the IFS's table
    /// and the poll waiting there returns.
    pub fn reset(&self) {
        let _ = self.io.call_blocking(|c| c.lease_reset());
    }

    /// An order found no open of `tag` in the library: what the table
    /// still has from this job there goes.
    pub fn none_open(&self, tag: u64) {
        let g = self.lock();
        self.send(&g, tag);
    }
}

/// The library's lease observer.
pub struct Observer(pub Arc<LeaseLink>);

impl LeaseObserver for Observer {
    fn leases(&self, share: &str, file_index: u64, leases: &[(LeaseKey, u32)]) {
        self.0.changed(share, file_index, leases);
    }
}
