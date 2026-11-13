// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mounted remote file system: its cache, its queue and the worker
//! threads that serve it. What the IFS asks on its own thread never
//! blocks: it is answered from the cache ([`Need::Ready`]), or the
//! missing piece is queued once (identical fetches share one ticket)
//! and the caller is told what to wait for ([`Need::Wait`]). Changes
//! are queued under the request's token and found by its next run
//! ([`RemoteMount::mutate`]).

// Block, handle, cache, queue: the short names are the domain's.
#![allow(clippy::many_single_char_names)]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::cache::{BState, Block, Cache, ListEnt};
use super::fs::{Mounted, RemoteFs};
use super::request::{self, Found};
use super::tickets;
use super::types::{Cred, FsStat, PathConf, RAttr, RHandle, RResult, RemoteError, RemoteInfo};
use super::work::{Answer, Mutation, Work};
use crate::errno::{Errno, Result};
use crate::lease::defer::Deferral;

/// The most runs of one request (each waits for work that brings it
/// further; a hard mount's outage is one wait, however long).
const MAX_RUNS: u32 = 10_000;

/// What a change does to a name, for [`RemoteMount::replayed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Touch {
    Make,
    Remove,
    Rename,
    Link,
}

/// What the storage service decides for every remote mount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteConfig {
    /// Worker threads per mount.
    pub threads: usize,
    /// Most bytes of clean data cached per mount.
    pub cache_bytes: usize,
    /// Most bytes of data waiting to be written per mount; a write past
    /// it waits for the write-back.
    pub dirty_bytes: usize,
    /// Blocks read ahead of sequential reads.
    pub readahead: u32,
    /// Full dirty blocks of one file that start write-behind.
    pub behind: u32,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            threads: 4,
            cache_bytes: 64 << 20,
            dirty_bytes: 16 << 20,
            readahead: 4,
            behind: 4,
        }
    }
}

/// What a mount reports about its server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteEvent {
    /// A call has run longer than the mount's `slow_after`.
    NotResponding {
        /// The mount.
        fs: u32,
        /// What is mounted.
        source: String,
    },
    /// A call was answered again.
    Responding {
        /// The mount.
        fs: u32,
        /// What is mounted.
        source: String,
    },
}

/// What the service turns events into (`QSYSOPR` messages).
pub type Events = Arc<dyn Fn(&RemoteEvent) + Send + Sync>;

/// What the cache answers: the thing, or the ticket of the work that
/// will bring it.
#[derive(Debug)]
pub(crate) enum Need<T> {
    Ready(T),
    Wait(u64),
}

/// The queue.
#[derive(Default)]
pub(super) struct Queue {
    pub items: VecDeque<(u64, Work)>,
    pub inflight: HashMap<Work, u64>,
    /// Calls in progress: worker number and start.
    pub running: HashMap<usize, Instant>,
    /// Work a helper job took: id, ticket, the work, the deadline.
    pub lent: HashMap<u64, (u64, Work, Instant)>,
    /// The generation of each lent write's block.
    pub gens: HashMap<u64, u64>,
    pub next_lent: u64,
    pub stop: bool,
}

/// What the mount's threads share.
pub(super) struct Core {
    pub fs: Box<dyn RemoteFs>,
    pub info: RemoteInfo,
    pub root: RHandle,
    pub pathconf: PathConf,
    pub id: AtomicU32,
    pub data_ccsid: u16,
    pub path_ccsid: u16,
    pub cache: Mutex<Cache>,
    pub q: Mutex<Queue>,
    pub cv: Condvar,
    pub events: Option<Events>,
    pub responding: AtomicBool,
    pub cfg: RemoteConfig,
    pub statfs: Mutex<Option<(RResult<FsStat>, Instant)>>,
    pub calls: AtomicU64,
}

impl Core {
    pub(super) fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn queue(&self) -> MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn bs(&self) -> u64 {
        u64::from(self.info.rsize.max(512))
    }
}

/// A remote file system mounted in the IFS.
pub struct RemoteMount {
    pub(super) core: Arc<Core>,
    /// `*NFS` ...
    pub kind: String,
    /// What was mounted as given (`host:/path`).
    pub mfs: String,
}

impl std::fmt::Debug for RemoteMount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteMount")
            .field("kind", &self.kind)
            .field("mfs", &self.mfs)
            .field("source", &self.core.info.source)
            .finish_non_exhaustive()
    }
}

/// A mount's root and limits, as its first calls found them.
pub(crate) struct Opened {
    pub mounted: Mounted,
    pub root: (RHandle, RAttr),
    pub pathconf: PathConf,
}

/// A mount's result while no request has claimed it: dropped unclaimed
/// (its request ended or was abandoned), the server is told the mount
/// ends, on a thread of its own.
pub(crate) struct Unclaimed(pub Option<RResult<Opened>>);

impl Drop for Unclaimed {
    fn drop(&mut self) {
        if let Some(Ok(o)) = self.0.take() {
            let _ = std::thread::Builder::new()
                .name("qs-remote-unmount".into())
                .spawn(move || {
                    if let Err(e) = o.mounted.fs.unmount() {
                        tracing::warn!(?e, "unclaimed remote mount not ended");
                    }
                });
        }
    }
}

impl RemoteMount {
    /// Start a mount: its cache with the root's attributes, its worker
    /// threads and its watchdog.
    pub(crate) fn start(
        o: Opened,
        kind: &str,
        mfs: &str,
        data_ccsid: u16,
        path_ccsid: u16,
        cfg: RemoteConfig,
        events: Option<Events>,
    ) -> Arc<Self> {
        let Opened {
            mounted,
            root,
            pathconf,
        } = o;
        let mut cache = Cache::new(mounted.info.attr_times, cfg.cache_bytes);
        cache.put_attr(&root.0, root.1);
        let core = Arc::new(Core {
            fs: mounted.fs,
            info: mounted.info,
            root: root.0,
            pathconf,
            id: AtomicU32::new(0),
            data_ccsid,
            path_ccsid,
            cache: Mutex::new(cache),
            q: Mutex::new(Queue::default()),
            cv: Condvar::new(),
            events,
            responding: AtomicBool::new(true),
            cfg,
            statfs: Mutex::new(None),
            calls: AtomicU64::new(0),
        });
        for n in 0..cfg.threads.max(1) {
            let c = Arc::clone(&core);
            let _ = std::thread::Builder::new()
                .name(format!("qs-remote-{n}"))
                .spawn(move || super::worker::serve(&c, n));
        }
        let c = Arc::clone(&core);
        let _ = std::thread::Builder::new()
            .name("qs-remote-watch".into())
            .spawn(move || super::worker::watch(&c));
        Arc::new(Self {
            core,
            kind: kind.to_string(),
            mfs: mfs.to_string(),
        })
    }

    /// The mount's id in the mount table.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.core.id.load(Ordering::Relaxed)
    }

    pub(crate) fn set_id(&self, id: u32) {
        self.core.id.store(id, Ordering::Relaxed);
    }

    /// What the mount said about itself.
    #[must_use]
    pub fn info(&self) -> &RemoteInfo {
        &self.core.info
    }

    /// The root's handle.
    #[must_use]
    pub fn root(&self) -> &RHandle {
        &self.core.root
    }

    /// Its limits.
    #[must_use]
    pub fn pathconf(&self) -> PathConf {
        self.core.pathconf
    }

    /// The CCSID data is tagged with (65535: binary).
    #[must_use]
    pub fn data_ccsid(&self) -> u16 {
        self.core.data_ccsid
    }

    /// The CCSID names are sent in.
    #[must_use]
    pub fn path_ccsid(&self) -> u16 {
        self.core.path_ccsid
    }

    /// Whether the server answers.
    #[must_use]
    pub fn responding(&self) -> bool {
        self.core.responding.load(Ordering::Relaxed)
    }

    /// Calls made into the remote file system so far.
    #[must_use]
    pub fn calls(&self) -> u64 {
        self.core.calls.load(Ordering::Relaxed)
    }

    /// End the worker threads once they have nothing left (a call in
    /// progress finishes first; nobody waits for it).
    pub(crate) fn shutdown(&self) {
        self.core.queue().stop = true;
        self.core.cv.notify_all();
    }

    /// A name as the server holds it.
    pub(crate) fn encode(&self, name: &str) -> Result<Vec<u8>> {
        match self.core.path_ccsid {
            ccsid::conv::UTF8 | ccsid::conv::BINARY => Ok(name.as_bytes().to_vec()),
            c => ccsid::conv::from_str(c, name).map_err(|_| Errno::ECONVERT),
        }
    }

    /// A server's name as text.
    pub(crate) fn decode(&self, name: &[u8]) -> String {
        match self.core.path_ccsid {
            ccsid::conv::UTF8 | ccsid::conv::BINARY => String::from_utf8_lossy(name).into_owned(),
            c => ccsid::conv::to_string(c, name)
                .unwrap_or_else(|_| String::from_utf8_lossy(name).into_owned()),
        }
    }

    /// Queue `w` unless the same work is queued or running: its ticket.
    pub(crate) fn push(&self, w: Work) -> u64 {
        let mut q = self.core.queue();
        if let Some(t) = q.inflight.get(&w) {
            return *t;
        }
        let t = tickets::issue();
        q.inflight.insert(w.clone(), t);
        q.items.push_back((t, w));
        drop(q);
        self.core.cv.notify_one();
        t
    }

    /// Turn what the cache answered into the operation's result: a
    /// wait defers the request (`EAGAIN`), or, on an `intr` mount after
    /// the job was interrupted, answers `EINTR`.
    pub(crate) fn settle<T>(&self, n: Need<RResult<T>>) -> Result<T> {
        match n {
            Need::Ready(r) => r.map_err(RemoteError::errno),
            Need::Wait(ticket) => {
                let tok = request::current();
                if let Some(t) = tok {
                    if self.core.info.intr && request::interrupted(t) {
                        return Err(Errno::EINTR);
                    }
                    if request::runs(t) > MAX_RUNS {
                        // Each run should get further; one that never ends
                        // is a fault, not a wait.
                        tracing::warn!(token = t.id, "remote request makes no progress");
                        return Err(Errno::EIO);
                    }
                    request::note_waiting();
                    crate::lease::defer::defer_remote(Deferral::Remote {
                        fs: self.id(),
                        ticket,
                        token: t.id,
                        until: Instant::now() + Duration::from_hours(24),
                    });
                }
                Err(Errno::EAGAIN)
            }
        }
    }

    /// When the request in progress began (now outside one).
    pub(crate) fn since() -> Instant {
        request::current().map_or_else(Instant::now, |t| t.since)
    }

    /// A cached error for `what` of `h`, fetched for this request.
    fn err(c: &Cache, h: &RHandle, what: &str) -> Option<RemoteError> {
        let i = c.inodes.get(h)?;
        if i.stale {
            return Some(RemoteError::Stale);
        }
        let (e, at) = i.errs.get(what)?;
        (*at >= Self::since()).then_some(*e)
    }

    /// Attributes, good within their time to live.
    pub(crate) fn attr(&self, h: &RHandle, cred: &Cred) -> Need<RResult<RAttr>> {
        let since = Self::since();
        let c = self.core.cache();
        if let Some(a) = c.attr(h, since) {
            return Need::Ready(Ok(a));
        }
        if let Some(e) = Self::err(&c, h, "attr") {
            return Need::Ready(Err(e));
        }
        drop(c);
        Need::Wait(self.push(Work::Getattr {
            h: h.clone(),
            cred: cred.clone(),
        }))
    }

    /// Attributes asked of the server for this request (close-to-open:
    /// an open sees what other clients wrote and closed).
    pub(crate) fn attr_now(&self, h: &RHandle, cred: &Cred) -> Need<RResult<RAttr>> {
        let since = Self::since();
        let c = self.core.cache();
        if let Some(a) = c.attr_since(h, since) {
            return Need::Ready(Ok(a));
        }
        if let Some(e) = Self::err(&c, h, "attr") {
            return Need::Ready(Err(e));
        }
        drop(c);
        Need::Wait(self.push(Work::Getattr {
            h: h.clone(),
            cred: cred.clone(),
        }))
    }

    /// A name in `dir`: its handle and attributes, `None` when absent.
    /// A miss queues the lookup of `name` and of `more` after it (the
    /// rest of a path), so a path takes one wait, not one per name.
    pub(crate) fn child(
        &self,
        dir: &RHandle,
        name: &[u8],
        more: &[Vec<u8>],
        cred: &Cred,
    ) -> Need<RResult<Option<(RHandle, RAttr)>>> {
        let since = Self::since();
        let c = self.core.cache();
        match c.name(dir, name, since) {
            Some(Err(e)) => Need::Ready(Err(e)),
            Some(Ok(None)) => Need::Ready(Ok(None)),
            Some(Ok(Some(h))) => {
                if let Some(a) = c.attr(&h, since) {
                    return Need::Ready(Ok(Some((h, a))));
                }
                if c.inodes.get(&h).is_some_and(|i| i.stale) {
                    // Looked up again: the name may name a new object.
                    drop(c);
                    self.core.cache().inode(dir).names.remove(name);
                    return Need::Wait(self.walk(dir, name, more, cred));
                }
                drop(c);
                if more.is_empty() {
                    return Need::Wait(self.push(Work::Getattr {
                        h,
                        cred: cred.clone(),
                    }));
                }
                // The rest of the path is likely as old: all again at once.
                Need::Wait(self.walk(dir, name, more, cred))
            }
            None => {
                drop(c);
                Need::Wait(self.walk(dir, name, more, cred))
            }
        }
    }

    fn walk(&self, dir: &RHandle, name: &[u8], more: &[Vec<u8>], cred: &Cred) -> u64 {
        let mut names = vec![name.to_vec()];
        names.extend(more.iter().cloned());
        self.push(Work::Walk {
            dir: dir.clone(),
            names,
            cred: cred.clone(),
        })
    }

    /// The caller's access bits ([`super::fs::access`]).
    pub(crate) fn access(&self, h: &RHandle, cred: &Cred) -> Need<RResult<u32>> {
        let since = Self::since();
        let c = self.core.cache();
        if let Some(b) = c.access(h, cred, since) {
            return Need::Ready(Ok(b));
        }
        if let Some(e) = Self::err(&c, h, &format!("access:{}", cred.uid)) {
            return Need::Ready(Err(e));
        }
        drop(c);
        Need::Wait(self.push(Work::Access {
            h: h.clone(),
            cred: cred.clone(),
        }))
    }

    /// A link's text, good while its attributes say it has not changed.
    pub(crate) fn readlink(&self, h: &RHandle, cred: &Cred) -> Need<RResult<Vec<u8>>> {
        let since = Self::since();
        let c = self.core.cache();
        if let Some(i) = c.inodes.get(h)
            && let (Some((t, ch)), Some(a)) = (&i.link, c.attr(h, since))
            && *ch == a.change
        {
            return Need::Ready(Ok(t.clone()));
        }
        if let Some(e) = Self::err(&c, h, "link") {
            return Need::Ready(Err(e));
        }
        drop(c);
        Need::Wait(self.push(Work::Readlink {
            h: h.clone(),
            cred: cred.clone(),
        }))
    }

    /// A directory's entries.
    pub(crate) fn listing(&self, dir: &RHandle, cred: &Cred) -> Need<RResult<Vec<ListEnt>>> {
        let since = Self::since();
        let c = self.core.cache();
        if let Some(l) = c.listing(dir, since) {
            return Need::Ready(Ok(l));
        }
        if let Some(e) = Self::err(&c, dir, "list") {
            return Need::Ready(Err(e));
        }
        drop(c);
        Need::Wait(self.push(Work::Readdir {
            dir: dir.clone(),
            cred: cred.clone(),
        }))
    }

    /// Capacity, asked at most every few seconds.
    pub(crate) fn statfs(&self, cred: &Cred) -> Need<RResult<FsStat>> {
        let since = Self::since();
        if let Some((r, at)) = &*self
            .core
            .statfs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            && super::cache::fresh(*at, Duration::from_secs(5), since)
        {
            return Need::Ready(*r);
        }
        Need::Wait(self.push(Work::Statfs { cred: cred.clone() }))
    }

    /// Read up to `len` bytes at `off` of a file `size` long (the size
    /// the caller sees). What is cached from `off` on comes back at
    /// once, perhaps short; a miss at `off` waits. Sequential reads
    /// queue the blocks ahead.
    pub(crate) fn read(
        &self,
        h: &RHandle,
        cred: &Cred,
        off: u64,
        len: usize,
        size: u64,
    ) -> Need<RResult<Vec<u8>>> {
        if off >= size || len == 0 {
            return Need::Ready(Ok(Vec::new()));
        }
        let bs = self.core.bs();
        let end = size.min(off.saturating_add(len as u64));
        let mut out = Vec::with_capacity(usize::try_from(end - off).unwrap_or(0));
        let mut missing: Option<u64> = None;
        let mut pos = off;
        let mut c = self.core.cache();
        while pos < end {
            let k = pos / bs;
            let from = usize::try_from(pos - k * bs).unwrap_or(0);
            let want_to = usize::try_from(end.min((k + 1) * bs) - k * bs).unwrap_or(0);
            let Some(b) = c.inodes.get(h).and_then(|i| i.blocks.get(&k)) else {
                missing = Some(k);
                break;
            };
            if !b.covers(from, want_to) {
                missing = Some(k);
                break;
            }
            let avail = b.data.len().min(want_to);
            if avail > from {
                out.extend_from_slice(&b.data[from..avail]);
            }
            if avail < want_to {
                // Past the data the server gave: holes read as zeros
                // up to the size the caller sees.
                out.resize(out.len() + (want_to - avail.max(from)), 0);
            }
            pos = k * bs + want_to as u64;
        }
        if out.is_empty()
            && let Some(k) = missing
            && let Some(e) = Self::err(&c, h, &format!("read:{k}"))
        {
            return Need::Ready(Err(e));
        }
        let seq = c.inodes.get(h).is_some_and(|i| i.next_seq == off);
        if !out.is_empty() {
            c.inode(h).next_seq = off + out.len() as u64;
        }
        // Blocks to fetch: the missing one and, reading on, those ahead.
        let first = missing.unwrap_or(pos / bs);
        let last_block = size.saturating_sub(1) / bs;
        let ahead = if seq || off == 0 {
            u64::from(self.core.cfg.readahead)
        } else {
            0
        };
        let mut want = Vec::new();
        let upto = if missing.is_some() {
            (end.saturating_sub(1) / bs).max(first) + ahead
        } else {
            first + ahead
        };
        for k in first..=upto.min(last_block) {
            let have = c
                .inodes
                .get(h)
                .and_then(|i| i.blocks.get(&k))
                .is_some_and(|b| b.base);
            if !have {
                want.push(k);
            }
        }
        drop(c);
        let mut first_wait = None;
        for k in want {
            let ticket = self.push(Work::Read {
                h: h.clone(),
                cred: cred.clone(),
                block: k,
            });
            if Some(k) == missing {
                first_wait = Some(ticket);
            }
        }
        if out.is_empty()
            && let Some(ticket) = first_wait
        {
            return Need::Wait(ticket);
        }
        Need::Ready(Ok(out))
    }

    /// Write `data` at `off` into the cache. It waits only when its
    /// blocks must be read first (a second piece of a block not read)
    /// or too much is waiting to be written.
    pub(crate) fn write(
        &self,
        h: &RHandle,
        cred: &Cred,
        off: u64,
        data: &[u8],
        size: u64,
    ) -> Need<RResult<()>> {
        let bs = self.core.bs();
        let end = off + data.len() as u64;
        let mut c = self.core.cache();
        if c.dirty_bytes() + data.len() > self.core.cfg.dirty_bytes {
            drop(c);
            return Need::Wait(self.push(Work::Mutate {
                m: Mutation::FlushAll,
                cred: cred.clone(),
                token: None,
            }));
        }
        // Blocks partly written before and not read: read them first.
        let mut need_read = None;
        let mut k = off / bs;
        while k * bs < end {
            let from = usize::try_from(off.max(k * bs) - k * bs).unwrap_or(0);
            let to = usize::try_from(end.min((k + 1) * bs) - k * bs).unwrap_or(0);
            if let Some(b) = c.inodes.get(h).and_then(|i| i.blocks.get(&k))
                && !b.base
                && let Some((a, z)) = b.dirty
                && (to < a || from > z)
                && k * bs < size
            {
                need_read = Some(k);
                break;
            }
            k += 1;
        }
        if let Some(k) = need_read {
            drop(c);
            return Need::Wait(self.push(Work::Read {
                h: h.clone(),
                cred: cred.clone(),
                block: k,
            }));
        }
        let change = c
            .inodes
            .get(h)
            .and_then(|i| i.attr)
            .map_or(0, |a| a.a.change);
        let mut full = Vec::new();
        let mut added = 0usize;
        let mut k = off / bs;
        while k * bs < end {
            let from = usize::try_from(off.max(k * bs) - k * bs).unwrap_or(0);
            let to = usize::try_from(end.min((k + 1) * bs) - k * bs).unwrap_or(0);
            let src =
                &data[usize::try_from(k * bs + from as u64 - off).unwrap_or(0)..][..to - from];
            let i = c.inode(h);
            let b = i.blocks.entry(k).or_insert_with(|| Block {
                data: Vec::new(),
                base: k * bs >= size,
                at: Instant::now(),
                change,
                state: BState::Dirty,
                dirty: None,
                epoch: 0,
                eof: false,
            });
            if b.data.len() < to {
                added += to - b.data.len();
                b.data.resize(to, 0);
            }
            b.data[from..to].copy_from_slice(src);
            b.dirty = Some(match b.dirty {
                Some((a, z)) => (a.min(from), z.max(to)),
                None => (from, to),
            });
            b.state = BState::Dirty;
            b.epoch += 1;
            if b.dirty == Some((0, usize::try_from(bs).unwrap_or(usize::MAX))) {
                full.push(k);
            }
            k += 1;
        }
        c.bytes += added;
        let i = c.inode(h);
        let seen = i.seen().map_or(size, |a| a.size).max(size);
        i.size_local = Some(seen.max(end));
        i.mtime_local = Some(crate::ops::now());
        let full_dirty = i
            .blocks
            .values()
            .filter(|b| {
                b.state == BState::Dirty && b.dirty.is_some_and(|(a, z)| a == 0 && z as u64 == bs)
            })
            .count();
        drop(c);
        if full_dirty >= self.core.cfg.behind as usize {
            for k in full {
                self.push(Work::WriteBlock {
                    h: h.clone(),
                    cred: cred.clone(),
                    block: k,
                });
            }
        }
        Need::Ready(Ok(()))
    }

    /// Whether `h` has data not yet on the server, or a write-back
    /// error not yet reported.
    pub(crate) fn has_dirty(&self, h: &RHandle) -> bool {
        self.core
            .cache()
            .inodes
            .get(h)
            .is_some_and(|i| i.has_dirty() || i.write_err.is_some())
    }

    /// A change, made once for the request in progress: its result
    /// when done, else the ticket to wait for. Outside a request that
    /// may defer nothing is queued (`EAGAIN`).
    pub(crate) fn mutate(&self, m: Mutation, cred: &Cred) -> Need<RResult<Answer>> {
        let Some(t) = request::current() else {
            return Need::Ready(Err(RemoteError::Errno(Errno::EAGAIN)));
        };
        if crate::lease::defer::pending() {
            // The run already waits: what it does now is not the request's.
            return Need::Ready(Err(RemoteError::Errno(Errno::EAGAIN)));
        }
        let key = request::occurrence(&format!("{}:{m:?}", self.id()));
        match request::slot(t, &key) {
            Found::Done(r) => Need::Ready(r),
            Found::InFlight(k) => Need::Wait(k),
            Found::None => {
                if !request::allowed() {
                    return Need::Ready(Err(RemoteError::Errno(Errno::EAGAIN)));
                }
                for n in self.touched(&m) {
                    request::note_name(&n, &key);
                }
                // The slot first: a worker may finish before push returns.
                let k = tickets::issue();
                request::queued(t, &key, k);
                let w = Work::Mutate {
                    m,
                    cred: cred.clone(),
                    token: Some((t.id, key)),
                };
                let mut q = self.core.queue();
                q.inflight.insert(w.clone(), k);
                q.items.push_back((k, w));
                drop(q);
                self.core.cv.notify_one();
                Need::Wait(k)
            }
        }
    }

    fn name_key(&self, touch: Touch, dir: &RHandle, name: &[u8]) -> String {
        format!("{touch:?}:{}:{dir:?}:{name:?}", self.id())
    }

    /// The names a change makes, removes, renames or links.
    fn touched(&self, m: &Mutation) -> Vec<String> {
        match m {
            Mutation::Create { dir, name, .. }
            | Mutation::Mkdir { dir, name, .. }
            | Mutation::Symlink { dir, name, .. }
            | Mutation::Mknod { dir, name, .. } => vec![self.name_key(Touch::Make, dir, name)],
            Mutation::Remove { dir, name, .. } | Mutation::Rmdir { dir, name, .. } => {
                vec![self.name_key(Touch::Remove, dir, name)]
            }
            Mutation::Rename { from_dir, from, .. } => {
                vec![self.name_key(Touch::Rename, from_dir, from)]
            }
            Mutation::Link { dir, name, .. } => vec![self.name_key(Touch::Link, dir, name)],
            _ => Vec::new(),
        }
    }

    /// What the request in progress already did to `name` in `dir` (a
    /// run after the change was queued): its checks, made before it,
    /// would see the change itself, so the operation takes this instead.
    pub(crate) fn replayed(
        &self,
        touch: Touch,
        dir: &RHandle,
        name: &[u8],
    ) -> Option<Need<RResult<Answer>>> {
        let t = request::current()?;
        let key = request::named(&self.name_key(touch, dir, name))?;
        match request::slot(t, &key) {
            Found::Done(r) => Some(Need::Ready(r)),
            Found::InFlight(k) => Some(Need::Wait(k)),
            Found::None => None,
        }
    }

    /// A change nobody waits for (the end of an open, a hidden file's
    /// removal at its last close, write-back at a job's end).
    pub(crate) fn background(&self, m: Mutation, cred: &Cred) {
        self.push(Work::Mutate {
            m,
            cred: cred.clone(),
            token: None,
        });
    }

    /// Remember that `h` was hidden as `name` in `dir` (removed while
    /// open).
    pub(crate) fn hide(&self, h: &RHandle, dir: &RHandle, name: &[u8]) {
        self.core.cache().inode(h).silly = Some((dir.clone(), name.to_vec()));
    }

    /// Where `h` was hidden, taken.
    pub(crate) fn take_hidden(&self, h: &RHandle) -> Option<(RHandle, Vec<u8>)> {
        self.core.cache().inodes.get_mut(h)?.silly.take()
    }

    /// Drop every cached attribute and name (tests: the server changed
    /// behind the mount's back).
    pub fn drop_cache(&self) {
        let mut c = self.core.cache();
        let keep: Vec<RHandle> = c
            .inodes
            .iter()
            .filter(|(_, i)| i.has_dirty() || i.silly.is_some())
            .map(|(h, _)| h.clone())
            .collect();
        c.inodes.retain(|h, _| keep.contains(h));
        for i in c.inodes.values_mut() {
            // What is left is asked again.
            i.attr = None;
            i.access.clear();
            i.names.clear();
            i.listing = None;
        }
        c.bytes = c
            .inodes
            .values()
            .flat_map(|i| i.blocks.values())
            .map(|b| b.data.len())
            .sum();
    }

    /// Bytes of clean and dirty data cached.
    #[must_use]
    pub fn cached_bytes(&self) -> usize {
        self.core.cache().bytes
    }
}
