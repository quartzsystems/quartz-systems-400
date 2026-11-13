// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mount's cache: per handle, the attributes with their time to
//! live (doubling while nothing changes, `acregmin`..`acregmax` and
//! `acdirmin`..`acdirmax`), the callers' access bits, a directory's
//! names (positive and negative) and its listing, a link's text, and
//! data blocks of `rsize` bytes, clean ones validated by the change
//! value they were read under and dirty ones waiting to be written.
//! An entry fetched since a request began is fresh for that request
//! whatever its time to live ([`fresh`]). Clean blocks past the byte
//! limit are dropped, oldest first.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use super::types::{AttrTimes, Cred, RAttr, RHandle, RKind, RemoteError};
use crate::backend::Nanos;

/// Whether an entry fetched `at` with time to live `ttl` is good for a
/// request that began `since`.
#[must_use]
pub fn fresh(at: Instant, ttl: Duration, since: Instant) -> bool {
    at >= since || at.elapsed() < ttl
}

/// Attributes and their life.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AttrEnt {
    pub a: RAttr,
    pub at: Instant,
    pub ttl: Duration,
}

/// A name in a directory: what it names (`None`: nothing) and the
/// directory's change value when it was looked up.
#[derive(Debug, Clone)]
pub(crate) struct NameEnt {
    pub to: Option<RHandle>,
    pub at: Instant,
    pub dir_change: u64,
    /// The lookup failed so (good only for the request that asked).
    pub err: Option<RemoteError>,
}

/// One entry of a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListEnt {
    pub name: Vec<u8>,
    pub fileid: u64,
    pub kind: Option<RKind>,
}

/// A directory's whole listing.
#[derive(Debug, Clone)]
pub(crate) struct Listing {
    pub ents: Vec<ListEnt>,
    pub at: Instant,
    pub dir_change: u64,
}

/// Where a block's bytes stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BState {
    /// As the server has them.
    Clean,
    /// Changed here, not yet sent.
    Dirty,
    /// Sent unstable under this verifier; kept until committed.
    Sent(u64),
}

/// A block of data.
#[derive(Debug, Clone)]
pub(crate) struct Block {
    /// Its bytes; shorter than the block size at end of file.
    pub data: Vec<u8>,
    /// The server's bytes are in `data` (else only `dirty` is good).
    pub base: bool,
    /// When read.
    pub at: Instant,
    /// The file's change value it was read under.
    pub change: u64,
    pub state: BState,
    /// The changed range within the block.
    pub dirty: Option<(usize, usize)>,
    /// Raised on every change, so a write that finishes after a newer
    /// change does not make it clean.
    pub epoch: u64,
    /// The server said end of file after it.
    pub eof: bool,
}

impl Block {
    /// Whether `[from, to)` of it can be read from here.
    pub(crate) fn covers(&self, from: usize, to: usize) -> bool {
        self.base || self.dirty.is_some_and(|(a, b)| a <= from && to <= b)
    }
}

/// One object.
#[derive(Debug, Default)]
pub(crate) struct Inode {
    pub attr: Option<AttrEnt>,
    pub access: HashMap<Cred, (u32, Instant)>,
    pub names: HashMap<Vec<u8>, NameEnt>,
    pub listing: Option<Listing>,
    pub link: Option<(Vec<u8>, u64)>,
    pub blocks: BTreeMap<u64, Block>,
    /// The size while writes are not yet on the server.
    pub size_local: Option<u64>,
    /// The modification time of local writes.
    pub mtime_local: Option<Nanos>,
    /// The first write-back error, for `fsync` or `close`.
    pub write_err: Option<RemoteError>,
    /// The server says it is gone.
    pub stale: bool,
    /// Removed while open: hidden under this name in this directory.
    pub silly: Option<(RHandle, Vec<u8>)>,
    /// Where sequential reads reached, for read-ahead.
    pub next_seq: u64,
    /// Fetches that failed, by what was asked, and when (good only for
    /// the request that asked).
    pub errs: HashMap<String, (RemoteError, Instant)>,
}

impl Inode {
    /// Whether it holds data not yet committed on the server.
    pub(crate) fn has_dirty(&self) -> bool {
        self.blocks.values().any(|b| b.state != BState::Clean)
    }

    /// The attributes as the caller sees them: local size and time
    /// while writes are pending.
    pub(crate) fn seen(&self) -> Option<RAttr> {
        let mut a = self.attr?.a;
        if let Some(s) = self.size_local {
            a.size = s;
        }
        if let Some(t) = self.mtime_local {
            a.mtime = a.mtime.max(t);
        }
        Some(a)
    }
}

/// The cache of one mount.
#[derive(Debug)]
pub(crate) struct Cache {
    pub inodes: HashMap<RHandle, Inode>,
    pub times: AttrTimes,
    /// Bytes of block data held.
    pub bytes: usize,
    /// Most bytes of clean data kept.
    pub limit: usize,
}

impl Cache {
    pub(crate) fn new(times: AttrTimes, limit: usize) -> Self {
        Self {
            inodes: HashMap::new(),
            times,
            bytes: 0,
            limit,
        }
    }

    pub(crate) fn inode(&mut self, h: &RHandle) -> &mut Inode {
        self.inodes.entry(h.clone()).or_default()
    }

    /// Attributes fresh for a request that began `since`.
    pub(crate) fn attr(&self, h: &RHandle, since: Instant) -> Option<RAttr> {
        let i = self.inodes.get(h)?;
        let e = i.attr?;
        fresh(e.at, e.ttl, since).then(|| i.seen()).flatten()
    }

    /// Attributes fetched since `since` (a forced revalidation, as at
    /// open).
    pub(crate) fn attr_since(&self, h: &RHandle, since: Instant) -> Option<RAttr> {
        let i = self.inodes.get(h)?;
        let e = i.attr?;
        (e.at >= since).then(|| i.seen()).flatten()
    }

    /// Take attributes from the server: the time to live doubles while
    /// the change value holds, and data read under another one goes.
    pub(crate) fn put_attr(&mut self, h: &RHandle, a: RAttr) {
        let times = self.times;
        let (min, max) = if a.kind == RKind::Dir {
            (times.dir_min, times.dir_max)
        } else {
            (times.reg_min, times.reg_max)
        };
        let i = self.inode(h);
        let ttl = match i.attr {
            Some(old) if old.a.change == a.change => (old.ttl * 2).clamp(min, max),
            _ => min,
        };
        let moved = i.attr.is_some_and(|old| old.a.change != a.change);
        let perms = i
            .attr
            .is_some_and(|old| (old.a.mode, old.a.uid, old.a.gid) != (a.mode, a.uid, a.gid));
        i.attr = Some(AttrEnt {
            a,
            at: Instant::now(),
            ttl,
        });
        i.stale = false;
        i.errs.remove("attr");
        if perms {
            i.access.clear();
        }
        let mut gone = 0;
        if moved && !i.has_dirty() {
            // Someone else changed it: what was read before is old.
            gone = i.blocks.values().map(|b| b.data.len()).sum();
            i.blocks.clear();
            i.link = None;
            i.listing = None;
            i.names.retain(|_, n| n.dir_change == a.change);
        } else if moved {
            // Our own writes moved it: keep the blocks, under the new value.
            for b in i.blocks.values_mut() {
                b.change = a.change;
            }
        }
        if let Some(s) = i.size_local
            && !i.has_dirty()
            && s == a.size
        {
            i.size_local = None;
            i.mtime_local = None;
        }
        self.bytes = self.bytes.saturating_sub(gone);
    }

    /// A name in `dir`, when known for a request that began `since`:
    /// `Ok(None)` when known absent.
    pub(crate) fn name(
        &self,
        dir: &RHandle,
        name: &[u8],
        since: Instant,
    ) -> Option<std::result::Result<Option<RHandle>, RemoteError>> {
        let d = self.inodes.get(dir)?;
        let n = d.names.get(name)?;
        if let Some(e) = n.err {
            return (n.at >= since).then_some(Err(e));
        }
        let ttl = d.attr.map_or(Duration::ZERO, |a| a.ttl);
        let dir_ok = d
            .attr
            .is_some_and(|a| fresh(a.at, a.ttl, since) && a.a.change == n.dir_change);
        (dir_ok || fresh(n.at, ttl, since)).then(|| Ok(n.to.clone()))
    }

    /// Record a name.
    pub(crate) fn put_name(&mut self, dir: &RHandle, name: &[u8], to: Option<RHandle>) {
        let d = self.inode(dir);
        let dir_change = d.attr.map_or(0, |a| a.a.change);
        d.names.insert(
            name.to_vec(),
            NameEnt {
                to,
                at: Instant::now(),
                dir_change,
                err: None,
            },
        );
    }

    /// Record a lookup that failed.
    pub(crate) fn name_err(&mut self, dir: &RHandle, name: &[u8], e: RemoteError) {
        self.inode(dir).names.insert(
            name.to_vec(),
            NameEnt {
                to: None,
                at: Instant::now(),
                dir_change: 0,
                err: Some(e),
            },
        );
    }

    /// Record a fetch that failed.
    pub(crate) fn put_err(&mut self, h: &RHandle, what: &str, e: RemoteError) {
        let i = self.inode(h);
        if e == RemoteError::Stale {
            i.stale = true;
        }
        i.errs.insert(what.to_string(), (e, Instant::now()));
    }

    /// A change made here to `dir`'s names: the listing goes, and names
    /// keep their standing under the directory's new change value.
    pub(crate) fn dir_changed(&mut self, dir: &RHandle, new: Option<RAttr>) {
        let d = self.inode(dir);
        d.listing = None;
        if let Some(a) = new {
            for n in d.names.values_mut() {
                n.dir_change = a.change;
            }
            d.attr = Some(AttrEnt {
                a,
                at: Instant::now(),
                ttl: d.attr.map_or(Duration::ZERO, |e| e.ttl),
            });
        } else {
            d.attr = None;
        }
    }

    /// The caller's access bits.
    pub(crate) fn access(&self, h: &RHandle, cred: &Cred, since: Instant) -> Option<u32> {
        let i = self.inodes.get(h)?;
        let ttl = i.attr.map_or(Duration::ZERO, |a| a.ttl);
        let (bits, at) = i.access.get(cred)?;
        fresh(*at, ttl, since).then_some(*bits)
    }

    /// A listing good for a request that began `since`.
    pub(crate) fn listing(&self, dir: &RHandle, since: Instant) -> Option<Vec<ListEnt>> {
        let d = self.inodes.get(dir)?;
        let l = d.listing.as_ref()?;
        let ttl = d.attr.map_or(Duration::ZERO, |a| a.ttl);
        let dir_ok = d
            .attr
            .is_some_and(|a| fresh(a.at, a.ttl, since) && a.a.change == l.dir_change);
        (dir_ok || fresh(l.at, ttl, since)).then(|| l.ents.clone())
    }

    /// Forget an object (removed, or stale).
    pub(crate) fn forget(&mut self, h: &RHandle) {
        if let Some(i) = self.inodes.remove(h) {
            let gone: usize = i.blocks.values().map(|b| b.data.len()).sum();
            self.bytes = self.bytes.saturating_sub(gone);
        }
    }

    /// Drop clean data past the limit, oldest first.
    pub(crate) fn trim(&mut self) {
        if self.bytes <= self.limit {
            return;
        }
        let mut clean: Vec<(Instant, RHandle, u64)> = self
            .inodes
            .iter()
            .flat_map(|(h, i)| {
                i.blocks
                    .iter()
                    .filter(|(_, b)| b.state == BState::Clean)
                    .map(move |(k, b)| (b.at, h.clone(), *k))
            })
            .collect();
        clean.sort_by_key(|(at, _, _)| *at);
        for (_, h, k) in clean {
            if self.bytes <= self.limit * 3 / 4 {
                break;
            }
            if let Some(i) = self.inodes.get_mut(&h)
                && let Some(b) = i.blocks.remove(&k)
            {
                self.bytes = self.bytes.saturating_sub(b.data.len());
            }
        }
    }

    /// Bytes of data waiting to be written.
    pub(crate) fn dirty_bytes(&self) -> usize {
        self.inodes
            .values()
            .flat_map(|i| i.blocks.values())
            .filter(|b| b.state != BState::Clean)
            .map(|b| b.data.len())
            .sum()
    }
}
