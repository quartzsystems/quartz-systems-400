// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mirrored set's write-intent map at run time.
//!
//! A region's bit is set, and made durable, before the first write
//! into it goes to any member; the write is counted while it runs. A
//! flush that began after the region's last write ended has made that
//! write durable on every member that takes writes, and from then the
//! bit may be cleared: lazily, once no write has touched the region for
//! the caller's delay, and in one write for every bit cleared together.
//!
//! For a member out of service the set may also keep a map of its own,
//! in a store the caller gave for that member: the set's bits when it
//! left, and every region set since. That map is never cleared until
//! the member is back in step.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::time::{Duration, Instant};

use tracing::warn;

use crate::UnitState;
use crate::intent::{IntentMap, IntentStore, MapKind, undo};
use crate::mirror::{RESUME_CHUNK, ResumeState};

/// How a mirrored set keeps its write-intent map.
pub struct Intent {
    /// The caller's magic, first in the kept bytes.
    pub magic: [u8; 8],
    /// Blocks per region: a multiple of the resume chunk
    /// ([`RESUME_CHUNK`]). 16 MiB by default.
    pub region_blocks: u64,
    /// How long a region stays set after its last write, so that a
    /// region written again and again is not set and cleared each time.
    /// Five seconds by default.
    pub clear_delay: Duration,
    /// The set is new: its members hold the same bytes and nothing is
    /// kept yet. The map is written clean. Otherwise it is read, and a
    /// map that is not there or not this set's takes every region as
    /// dirty.
    pub fresh: bool,
    /// The set's map.
    pub store: Box<dyn IntentStore>,
    /// For each member, where its own map is kept while it is out of
    /// service; `None` (or a short list) for a member that has none and
    /// comes back by a full copy.
    pub away: Vec<Option<Box<dyn IntentStore>>>,
}

impl Intent {
    /// The defaults: 16 MiB regions, five seconds, no member maps.
    #[must_use]
    pub fn new(magic: [u8; 8], store: Box<dyn IntentStore>) -> Self {
        Self {
            magic,
            region_blocks: crate::intent::DEFAULT_REGION_BYTES / crate::PAGE_SIZE as u64,
            clear_delay: Duration::from_secs(5),
            fresh: false,
            store,
            away: Vec::new(),
        }
    }
}

/// What the map has done, for displays and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IntentStats {
    /// Regions in the set.
    pub regions: u64,
    /// Regions set now.
    pub dirty: u64,
    /// Durable writes that set bits.
    pub sets: u64,
    /// Durable writes that cleared bits.
    pub clears: u64,
}

/// A region whose bit is set.
struct Region {
    /// Writes running in it.
    inflight: u32,
    /// The flush epoch its last write ended in.
    last: u64,
    /// When its last write ended; `None` before any did.
    at: Option<Instant>,
}

/// A member's own map.
enum Away {
    /// No store for it.
    None,
    /// A store, not in use: the member is in service.
    Idle(Box<dyn IntentStore>),
    /// Kept while the member is out of service.
    Keeping(IntentMap, Box<dyn IntentStore>),
    /// Given up (not kept when it should have been, or not readable):
    /// the member comes back by a full copy.
    Lost(Box<dyn IntentStore>),
}

/// The map of one set.
pub(crate) struct Tracker {
    magic: [u8; 8],
    id: [u8; 16],
    blocks: u64,
    delay: Duration,
    map: IntentMap,
    store: Box<dyn IntentStore>,
    /// A clear may have reached the kept map while memory has the bit
    /// set: the whole map is written before the next bit is set.
    stale: bool,
    regions: BTreeMap<u64, Region>,
    epoch: u64,
    flushed: u64,
    /// Regions to copy between active members after an unclean stop.
    pending: BTreeSet<u64>,
    /// Regions set when the set was opened.
    crash: Vec<u64>,
    away: Vec<Away>,
    stats: IntentStats,
}

fn out_of_service(s: UnitState) -> bool {
    matches!(
        s,
        UnitState::Suspended | UnitState::Missing | UnitState::NotOperational
    )
}

impl Tracker {
    /// The map of a set of `blocks` blocks whose members are `members`
    /// (identity and state after open's checks); `active` members are
    /// active and attached.
    pub(crate) fn open(
        id: [u8; 16],
        blocks: u64,
        mut intent: Intent,
        members: &[([u8; 16], UnitState)],
        active: usize,
    ) -> io::Result<Self> {
        let rb = intent.region_blocks;
        let magic = intent.magic;
        let clean = IntentMap::new(MapKind::Set, id, [0; 16], blocks, rb);
        let kept = if intent.fresh {
            None
        } else {
            let bytes = intent.store.read_all()?;
            let m = IntentMap::decode(&magic, &bytes)
                .filter(|m| m.kind == MapKind::Set && m.fits(id, [0; 16], blocks, rb));
            if m.is_none() {
                warn!("mirrored set: no write-intent map kept; every region is taken as dirty");
            }
            Some(m)
        };
        let map = match kept {
            Some(Some(m)) => m,
            Some(None) => {
                let mut m = clean;
                m.set_all();
                intent.store.write(0, &m.encode(&magic))?;
                m
            }
            None => {
                intent.store.write(0, &clean.encode(&magic))?;
                clean
            }
        };
        let crash: Vec<u64> = map.dirty().collect();
        let regions = crash
            .iter()
            .map(|&r| {
                (
                    r,
                    Region {
                        inflight: 0,
                        last: 0,
                        at: None,
                    },
                )
            })
            .collect();
        let pending = if active >= 2 {
            crash.iter().copied().collect()
        } else {
            BTreeSet::new()
        };
        let mut away = Vec::with_capacity(members.len());
        for (i, &(uuid, state)) in members.iter().enumerate() {
            let store = intent.away.get_mut(i).and_then(Option::take);
            away.push(match store {
                None => Away::None,
                Some(s) if !out_of_service(state) => Away::Idle(s),
                Some(mut s) => {
                    let m = s
                        .read_all()
                        .ok()
                        .and_then(|b| IntentMap::decode(&magic, &b));
                    match m {
                        Some(m)
                            if matches!(m.kind, MapKind::Away | MapKind::AwayResuming)
                                && m.fits(id, uuid, blocks, rb) =>
                        {
                            Away::Keeping(m, s)
                        }
                        _ => {
                            warn!("mirrored set: member {i} has no map of its own kept");
                            Away::Lost(s)
                        }
                    }
                }
            });
        }
        let stats = IntentStats {
            regions: map.regions,
            dirty: map.count(),
            sets: 0,
            clears: 0,
        };
        Ok(Self {
            magic,
            id,
            blocks,
            delay: intent.clear_delay,
            map,
            store: intent.store,
            stale: false,
            regions,
            epoch: 1,
            flushed: 0,
            pending,
            crash,
            away,
            stats,
        })
    }

    /// The regions blocks `lba..lba + count` touch, first and last.
    pub(crate) fn regions_of(&self, lba: u64, count: u64) -> (u64, u64) {
        let rb = self.map.region_blocks;
        (lba / rb, (lba + count.max(1) - 1) / rb)
    }

    /// Before a write to regions `lo..=hi` goes to any member: their
    /// bits set and durable, the write counted.
    pub(crate) fn begin(&mut self, lo: u64, hi: u64) -> io::Result<()> {
        if self.stale {
            self.store.write(0, &self.map.encode(&self.magic))?;
            self.stale = false;
        }
        let mut new = Vec::new();
        for r in lo..=hi {
            self.regions
                .entry(r)
                .or_insert(Region {
                    inflight: 0,
                    last: 0,
                    at: None,
                })
                .inflight += 1;
            if self.map.set(r) {
                new.push(r);
            }
        }
        if let (Some(&a), Some(&b)) = (new.first(), new.last()) {
            let (off, bytes) = self.map.span(a, b);
            if let Err(e) = self.store.write(off, bytes) {
                for &r in &new {
                    self.map.clear(r);
                }
                self.forget(lo, hi);
                return Err(e);
            }
            self.stats.sets += 1;
        }
        for (i, a) in self.away.iter_mut().enumerate() {
            let Away::Keeping(m, s) = a else { continue };
            let set: Vec<u64> = (lo..=hi).filter(|&r| m.set(r)).collect();
            if let (Some(&a0), Some(&b0)) = (set.first(), set.last()) {
                let (off, bytes) = m.span(a0, b0);
                if let Err(e) = s.write(off, bytes) {
                    warn!("mirrored set: the map of member {i} is given up: {e}");
                    let Away::Keeping(_, s) = std::mem::replace(a, Away::None) else {
                        unreachable!()
                    };
                    *a = Away::Lost(s);
                }
            }
        }
        Ok(())
    }

    /// Undo the counting of a write that did not start.
    fn forget(&mut self, lo: u64, hi: u64) {
        for r in lo..=hi {
            if let Some(g) = self.regions.get_mut(&r) {
                g.inflight -= 1;
                if g.inflight == 0 && !self.map.is_set(r) {
                    self.regions.remove(&r);
                }
            }
        }
    }

    /// A write to regions `lo..=hi` has ended, done or failed.
    pub(crate) fn end(&mut self, lo: u64, hi: u64) {
        let now = Instant::now();
        for r in lo..=hi {
            if let Some(g) = self.regions.get_mut(&r) {
                g.inflight -= 1;
                g.last = self.epoch;
                g.at = Some(now);
            }
        }
    }

    /// A flush is about to be sent to the members: its epoch.
    pub(crate) fn flush_begin(&mut self) -> u64 {
        let e = self.epoch;
        self.epoch += 1;
        e
    }

    /// The flush of epoch `e` reached every member that takes writes.
    pub(crate) fn flush_done(&mut self, e: u64) {
        self.flushed = self.flushed.max(e);
    }

    /// Clear the bits of regions whose writes are durable everywhere
    /// and that have not been written for the delay (any time, when
    /// `force`), in one write. The regions cleared.
    pub(crate) fn sweep(&mut self, force: bool) -> io::Result<u64> {
        let now = Instant::now();
        let clear: Vec<u64> = self
            .regions
            .iter()
            .filter(|(r, g)| {
                g.inflight == 0
                    && g.last <= self.flushed
                    && !self.pending.contains(r)
                    && (force || g.at.is_none_or(|t| now.duration_since(t) >= self.delay))
            })
            .map(|(r, _)| *r)
            .collect();
        let (Some(&a), Some(&b)) = (clear.first(), clear.last()) else {
            return Ok(0);
        };
        for &r in &clear {
            self.map.clear(r);
        }
        let (off, bytes) = self.map.span(a, b);
        match self.store.write(off, bytes) {
            Ok(()) => {
                for r in &clear {
                    self.regions.remove(r);
                }
                self.stats.clears += 1;
                Ok(clear.len() as u64)
            }
            Err(e) => {
                for &r in &clear {
                    self.map.set(r);
                }
                self.stale = true;
                Err(e)
            }
        }
    }

    /// The statistics now.
    pub(crate) fn stats(&self) -> IntentStats {
        IntentStats {
            dirty: self.map.count(),
            ..self.stats
        }
    }

    /// The regions set now.
    pub(crate) fn dirty(&self) -> Vec<u64> {
        self.map.dirty().collect()
    }

    /// Chunks per region.
    fn per_region(&self) -> u64 {
        self.map.region_blocks / RESUME_CHUNK
    }

    /// The chunks of region `r`.
    fn chunks_of(&self, r: u64) -> std::ops::Range<u64> {
        let chunks = self.blocks.div_ceil(RESUME_CHUNK);
        let n = self.per_region();
        (r * n).min(chunks)..((r + 1) * n).min(chunks)
    }

    /// Whether any of regions `lo..=hi` waits for the copy between
    /// active members.
    pub(crate) fn pending_in(&self, lo: u64, hi: u64) -> bool {
        self.pending.range(lo..=hi).next().is_some()
    }

    /// Whether a copy between active members waits.
    pub(crate) fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The map of chunks for the copy between active members: every
    /// chunk done but those of the regions waiting.
    pub(crate) fn resync_map(&self) -> ResumeState {
        let mut s = ResumeState::new(self.id, [0; 16], self.blocks);
        s.done.fill(0xFF);
        for &r in &self.pending {
            for c in self.chunks_of(r) {
                undo(&mut s, c);
            }
        }
        s
    }

    /// The copy has passed every chunk before `next`: the regions
    /// behind it are in step.
    pub(crate) fn resynced_below(&mut self, next: u64) {
        let r = next / self.per_region();
        self.pending = self.pending.split_off(&r);
    }

    /// Nothing waits for the copy any more (it ended, or no member is
    /// left to copy to).
    pub(crate) fn resynced(&mut self) {
        self.pending.clear();
    }

    /// Mark the chunks of the regions set at open, and of the member's
    /// own map, not done in a resume's map that a journal kept: the
    /// member may have missed writes in them since they were copied.
    pub(crate) fn redo(&self, member: usize, state: &mut ResumeState) {
        let mut regions: BTreeSet<u64> = self.crash.iter().copied().collect();
        if let Some(Away::Keeping(m, _)) = self.away.get(member) {
            regions.extend(m.dirty());
        }
        for r in regions {
            for c in self.chunks_of(r) {
                undo(state, c);
            }
        }
    }

    /// A new resume's map for a member back in service, when its own
    /// map was kept from when it left in step: only its regions to
    /// copy. `None` for a full copy.
    pub(crate) fn guide(&self, member: usize, unit: [u8; 16]) -> Option<ResumeState> {
        let Some(Away::Keeping(m, _)) = self.away.get(member) else {
            return None;
        };
        if m.kind != MapKind::Away {
            return None;
        }
        let mut s = ResumeState::new(self.id, unit, self.blocks);
        s.done.fill(0xFF);
        for r in m.dirty() {
            for c in self.chunks_of(r) {
                undo(&mut s, c);
            }
        }
        Some(s)
    }

    /// Member `member` stopped taking writes: keep its map from the
    /// set's bits now. `resuming` when it was not in step.
    pub(crate) fn left(&mut self, member: usize, uuid: [u8; 16], resuming: bool) {
        let Some(slot) = self.away.get_mut(member) else {
            return;
        };
        if !matches!(slot, Away::Idle(_)) {
            return;
        }
        let Away::Idle(mut s) = std::mem::replace(slot, Away::None) else {
            unreachable!()
        };
        let kind = if resuming {
            MapKind::AwayResuming
        } else {
            MapKind::Away
        };
        let mut m = IntentMap::new(kind, self.id, uuid, self.blocks, self.map.region_blocks);
        m.merge(&self.map);
        *slot = match s.write(0, &m.encode(&self.magic)) {
            Ok(()) => Away::Keeping(m, s),
            Err(e) => {
                warn!("mirrored set: no map kept for member {member}: {e}");
                Away::Lost(s)
            }
        };
    }

    /// Member `member` is in step again, or another disk took its
    /// place: its map is given up.
    pub(crate) fn back(&mut self, member: usize) {
        let Some(slot) = self.away.get_mut(member) else {
            return;
        };
        let s = match std::mem::replace(slot, Away::None) {
            Away::None => return,
            Away::Idle(s) => {
                *slot = Away::Idle(s);
                return;
            }
            Away::Keeping(_, s) | Away::Lost(s) => s,
        };
        let off = IntentMap::new(
            MapKind::Off,
            self.id,
            [0; 16],
            self.blocks,
            self.map.region_blocks,
        );
        let mut s = s;
        if let Err(e) = s.write(0, &off.header(&self.magic)) {
            warn!("mirrored set: the map of member {member} not given up: {e}");
        }
        *slot = Away::Idle(s);
    }

    /// Whether a member's map was given up while it was away.
    pub(crate) fn lost(&self, member: usize) -> bool {
        matches!(self.away.get(member), Some(Away::Lost(_)))
    }
}
