// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parity set's maps of its members out of service.
//!
//! A member that fails or leaves gets a map of its own: the regions
//! written but not yet flushed at that moment (and, if it was being
//! rebuilt without a map, every region from its mark on), then every
//! region written while it is away, each bit durable before the write
//! goes to any member. Nothing is cleared until the member is in step
//! again. A member that comes back is rebuilt in those regions only.
//!
//! A map is a region of the layout's blocks (`Geometry::blocks`), so one
//! region is the same rows and parity pages on every member.
//!
//! The rule that keeps a map honest: whenever the kept bytes are a valid
//! map for the member, every stripe outside its regions holds the
//! member's page in step with the rest. So the bits are written before
//! the header that makes them a map, a write whose bit cannot be kept
//! is refused unless the map can be given up, and a map is given up by
//! writing its header off.

use std::io;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use blockdev::BlockDev;
use tracing::warn;

use crate::intent::{HEADER_LEN, IntentMap, IntentStore, MapKind};
use crate::parity::Health;

/// How a parity set keeps a map for each member out of service.
pub struct ParityIntent {
    /// The caller's magic, first in the kept bytes.
    pub magic: [u8; 8],
    /// Blocks of the layout ([`Geometry::blocks`](crate::parity::Geometry))
    /// per region: 1,024 by default, which is 1,024 × n pages of each
    /// member (16 MiB of each member of a set of four).
    pub region_blocks: u64,
    /// For each member, where its map is kept; `None` (or a short list)
    /// for a member that comes back by a full rebuild.
    pub away: Vec<Option<Box<dyn IntentStore>>>,
}

impl ParityIntent {
    /// The defaults: 1,024 blocks a region, no stores yet.
    #[must_use]
    pub fn new(magic: [u8; 8]) -> Self {
        Self {
            magic,
            region_blocks: 1024,
            away: Vec::new(),
        }
    }
}

/// A member's map.
enum Away {
    /// No store for it.
    None,
    /// A store holding no valid map: the member is in service, or is
    /// being rebuilt whole.
    Idle(Box<dyn IntentStore>),
    /// Kept. `stale`: the kept bytes may lack bits memory has (a write
    /// of them failed); they are written whole before the next bit.
    Keeping {
        map: IntentMap,
        store: Box<dyn IntentStore>,
        stale: bool,
    },
    /// Given up while the member is failed: no valid map is kept, and it
    /// comes back by a full rebuild.
    Lost(Box<dyn IntentStore>),
}

/// The maps of one set.
pub(crate) struct Watch {
    magic: [u8; 8],
    id: [u8; 16],
    blocks: u64,
    rb: u64,
    /// Regions written since the last flush that completed.
    unflushed: IntentMap,
    away: Vec<Away>,
    uuids: Vec<[u8; 16]>,
    /// Each member's health as of the set's last look.
    pub(crate) health: Vec<Health>,
    /// Whether a member's partial-parity slots were zeroed for its
    /// rebuild from a map.
    pub(crate) zeroed: Vec<bool>,
    /// The members' devices, for `detach`.
    devs: Vec<Option<Arc<Slot>>>,
}

/// What a store held at open.
pub(crate) enum Kept {
    /// No store.
    NoStore,
    /// A store, no valid map in it.
    Nothing(Box<dyn IntentStore>),
    /// A valid map of this member.
    Map(IntentMap, Box<dyn IntentStore>),
}

impl Kept {
    pub(crate) fn has_map(&self) -> bool {
        matches!(self, Kept::Map(..))
    }
}

fn off(magic: [u8; 8], id: [u8; 16], blocks: u64, rb: u64) -> [u8; HEADER_LEN] {
    IntentMap::new(MapKind::Off, id, [0; 16], blocks, rb).header(&magic)
}

/// Write a whole map: the bits, then the header that makes them one.
fn write_map(store: &mut dyn IntentStore, map: &IntentMap, magic: [u8; 8]) -> io::Result<()> {
    let (off, bits) = map.span(0, map.regions - 1);
    store.write(off, bits)?;
    store.write(0, &map.header(&magic))
}

impl Watch {
    /// What member `uuid`'s store holds. A store that cannot be read is
    /// given up; one that cannot be given up either refuses the open.
    pub(crate) fn read_kept(
        magic: [u8; 8],
        id: [u8; 16],
        uuid: [u8; 16],
        blocks: u64,
        rb: u64,
        store: Option<Box<dyn IntentStore>>,
    ) -> io::Result<Kept> {
        let Some(mut s) = store else {
            return Ok(Kept::NoStore);
        };
        let bytes = match s.read_all() {
            Ok(b) => b,
            Err(e) => {
                warn!("parity set: a member's map cannot be read: {e}");
                s.write(0, &off(magic, id, blocks, rb))?;
                return Ok(Kept::Nothing(s));
            }
        };
        Ok(match IntentMap::decode(&magic, &bytes) {
            Some(m) if m.kind == MapKind::Away && m.fits(id, uuid, blocks, rb) => Kept::Map(m, s),
            _ => Kept::Nothing(s),
        })
    }

    /// The maps of a set of `blocks` blocks, from what each store held,
    /// with the regions in flight when it stopped (the partial-parity
    /// slots' stripes) taken as unflushed.
    #[allow(clippy::too_many_arguments)] // one call, at open
    pub(crate) fn new(
        magic: [u8; 8],
        id: [u8; 16],
        blocks: u64,
        rb: u64,
        kept: Vec<Kept>,
        uuids: Vec<[u8; 16]>,
        health: Vec<Health>,
        in_flight: IntentMap,
    ) -> Self {
        let n = kept.len();
        let away = kept
            .into_iter()
            .map(|k| match k {
                Kept::NoStore => Away::None,
                Kept::Nothing(s) => Away::Idle(s),
                Kept::Map(map, store) => Away::Keeping {
                    map,
                    store,
                    stale: false,
                },
            })
            .collect();
        Self {
            magic,
            id,
            blocks,
            rb,
            unflushed: in_flight,
            away,
            uuids,
            health,
            zeroed: vec![false; n],
            devs: vec![None; n],
        }
    }

    /// An empty map of regions, for counting what is unflushed.
    pub(crate) fn scratch(id: [u8; 16], blocks: u64, rb: u64) -> IntentMap {
        IntentMap::new(MapKind::Off, id, [0; 16], blocks, rb)
    }

    /// Blocks per region.
    pub(crate) fn region_blocks(&self) -> u64 {
        self.rb
    }

    /// Wrap member `member`'s device so that a failure keeps its map
    /// before the set goes on without it.
    pub(crate) fn wrap(
        watch: &Arc<Mutex<Watch>>,
        member: usize,
        dev: Box<dyn BlockDev>,
    ) -> Box<dyn BlockDev> {
        let slot = Arc::new(Slot(Mutex::new(Some(dev))));
        lock(watch).devs[member] = Some(Arc::clone(&slot));
        Box::new(Watched {
            slot,
            member,
            watch: Arc::clone(watch),
        })
    }

    /// The device given for `member`, unwrapped, once the set let it go.
    pub(crate) fn take_dev(&mut self, member: usize) -> Option<Box<dyn BlockDev>> {
        let slot = self.devs[member].take()?;
        let mut g = slot.0.lock().unwrap_or_else(PoisonError::into_inner);
        g.take()
    }

    /// Before a write to regions `regions` (first, last) goes to any
    /// member: counted as unflushed, and set durably in every map kept.
    /// An error when a map lacks a bit and cannot be given up.
    pub(crate) fn begin(&mut self, regions: &[(u64, u64)]) -> io::Result<()> {
        for &(lo, hi) in regions {
            for r in lo..=hi {
                self.unflushed.set(r);
            }
        }
        for i in 0..self.away.len() {
            let Away::Keeping { map, store, stale } = &mut self.away[i] else {
                continue;
            };
            let mut new: Option<(u64, u64)> = None;
            for &(lo, hi) in regions {
                for r in lo..=hi {
                    if map.set(r) {
                        new = Some(new.map_or((r, r), |(a, b)| (a.min(r), b.max(r))));
                    }
                }
            }
            let res = if *stale {
                write_map(store.as_mut(), map, self.magic)
            } else if let Some((a, b)) = new {
                let (off, bytes) = map.span(a, b);
                store.write(off, bytes)
            } else {
                Ok(())
            };
            match res {
                Ok(()) => *stale = false,
                Err(e) => {
                    *stale = true;
                    if store
                        .write(0, &off(self.magic, self.id, self.blocks, self.rb))
                        .is_err()
                    {
                        warn!("parity set: member {i}'s map can be neither kept nor given up");
                        return Err(e);
                    }
                    warn!("parity set: member {i}'s map is given up: {e}");
                    self.lose(i);
                }
            }
        }
        Ok(())
    }

    /// Member `i`'s map is given up (its header is off).
    fn lose(&mut self, i: usize) {
        let s = match std::mem::replace(&mut self.away[i], Away::None) {
            Away::Idle(s) | Away::Lost(s) | Away::Keeping { store: s, .. } => s,
            Away::None => return,
        };
        self.away[i] = if self.health[i] == Health::Failed {
            Away::Lost(s)
        } else {
            Away::Idle(s)
        };
    }

    /// A flush completed: what was written is on every member that
    /// takes writes.
    pub(crate) fn flushed(&mut self) {
        self.unflushed = Self::scratch(self.id, self.blocks, self.rb);
    }

    /// Member `member` stopped taking writes, or is about to: keep its
    /// map from what is unflushed, and from its rebuild's mark on when it
    /// was being rebuilt whole. A map already kept goes on as it is.
    pub(crate) fn left(&mut self, member: usize) {
        if !matches!(self.away[member], Away::Idle(_)) {
            return;
        }
        let Away::Idle(mut store) = std::mem::replace(&mut self.away[member], Away::None) else {
            unreachable!()
        };
        let mut map = IntentMap::new(
            MapKind::Away,
            self.id,
            self.uuids[member],
            self.blocks,
            self.rb,
        );
        map.merge(&self.unflushed);
        if let Health::Rebuilding(mark) = self.health[member] {
            for r in mark / self.rb..map.regions {
                map.set(r);
            }
        }
        let (off_bits, bits) = map.span(0, map.regions - 1);
        self.away[member] = if let Err(e) = store.write(off_bits, bits) {
            // The header still says no map.
            warn!("parity set: no map kept for member {member}: {e}");
            Away::Lost(store)
        } else if let Err(e) = store.write(0, &map.header(&self.magic)) {
            warn!("parity set: no map kept for member {member}: {e}");
            if store
                .write(0, &off(self.magic, self.id, self.blocks, self.rb))
                .is_ok()
            {
                Away::Lost(store)
            } else {
                // The kept bytes are a whole map or none; the next write
                // writes it again before it goes on.
                Away::Keeping {
                    map,
                    store,
                    stale: true,
                }
            }
        } else {
            Away::Keeping {
                map,
                store,
                stale: false,
            }
        };
    }

    /// Whether a map is kept for `member`.
    pub(crate) fn keeping(&self, member: usize) -> bool {
        matches!(self.away[member], Away::Keeping { .. })
    }

    /// The regions of `member`'s map, if one is kept.
    pub(crate) fn regions(&self, member: usize) -> Option<Vec<u64>> {
        match &self.away[member] {
            Away::Keeping { map, .. } => Some(map.dirty().collect()),
            _ => None,
        }
    }

    /// The blocks to rebuild next for `member` from block `from`: the
    /// rest of the first region of its map there, `None` past the last.
    pub(crate) fn next_blocks(&self, member: usize, from: u64) -> Option<(u64, u64)> {
        let Away::Keeping { map, .. } = &self.away[member] else {
            return None;
        };
        let r = map.next_set(from / self.rb)?;
        let start = from.max(r * self.rb);
        let stop = ((r + 1) * self.rb).min(self.blocks);
        (start < stop).then_some((start, stop))
    }

    /// Member `member` is in step again: its map is given up. One that
    /// cannot be is kept as it is, and every write goes on setting it.
    fn back(&mut self, member: usize) {
        match &mut self.away[member] {
            Away::Keeping { store, .. } => {
                match store.write(0, &off(self.magic, self.id, self.blocks, self.rb)) {
                    Ok(()) => self.lose(member),
                    Err(e) => warn!("parity set: member {member}'s map not given up: {e}"),
                }
            }
            Away::Lost(_) => self.lose(member),
            Away::None | Away::Idle(_) => {}
        }
    }

    /// Another disk took `member`'s place: its map is given up and the
    /// new disk is rebuilt whole.
    pub(crate) fn replaced(&mut self, member: usize, uuid: [u8; 16]) {
        if let Away::Keeping { store, .. } = &mut self.away[member] {
            // A map left behind names the old disk: not this one's.
            let _ = store.write(0, &off(self.magic, self.id, self.blocks, self.rb));
        }
        self.uuids[member] = uuid;
        self.zeroed[member] = false;
        self.health[member] = Health::Rebuilding(0);
        self.lose(member);
    }

    /// The set's members as they are now: each one's map follows.
    pub(crate) fn settle(&mut self, health: &[Health]) {
        for (i, &h) in health.iter().enumerate() {
            self.health[i] = h;
            match (h, &self.away[i]) {
                (Health::Ok, Away::Keeping { .. })
                | (Health::Ok | Health::Rebuilding(_), Away::Lost(_)) => {
                    self.back(i);
                }
                (Health::Failed, Away::Idle(_)) => {
                    // Not seen leaving: no map is kept for it.
                    warn!("parity set: member {i} failed with no map kept");
                    self.lose(i);
                }
                _ => {}
            }
        }
    }

    /// A device of the set failed I/O: keep its member's map before the
    /// set goes on without it.
    fn on_error(&mut self, member: usize) {
        if self.health[member] != Health::Failed {
            self.left(member);
        }
    }
}

/// The watch, locked.
pub(crate) fn lock(w: &Mutex<Watch>) -> MutexGuard<'_, Watch> {
    w.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A member's device as the set gave it.
struct Slot(Mutex<Option<Box<dyn BlockDev>>>);

/// A member's device under the set's watch.
struct Watched {
    slot: Arc<Slot>,
    member: usize,
    watch: Arc<Mutex<Watch>>,
}

impl Watched {
    fn with<T>(&self, f: impl FnOnce(&dyn BlockDev) -> blockdev::Result<T>) -> blockdev::Result<T> {
        let g = self.slot.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(dev) = g.as_deref() else {
            return Err(crate::set::no_member("member", 0));
        };
        let r = f(dev);
        drop(g);
        if r.is_err() {
            lock(&self.watch).on_error(self.member);
        }
        r
    }
}

impl BlockDev for Watched {
    fn block_size(&self) -> u32 {
        blockdev::BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        let g = self.slot.0.lock().unwrap_or_else(PoisonError::into_inner);
        g.as_deref().map_or(0, BlockDev::blocks)
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        self.with(|d| d.read(lba, buf))
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        self.with(|d| d.write(lba, buf))
    }

    fn flush(&self) -> blockdev::Result<()> {
        self.with(|d| d.flush())
    }

    fn discard(&self, lba: u64, count: u64) -> blockdev::Result<()> {
        self.with(|d| d.discard(lba, count))
    }
}
