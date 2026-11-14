// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the parity map tests share: devices that log their reads and
//! can die, the set's maps in memory, which blocks a rebuild read.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use blockdev::{BlockDev, BlockError, MemBlockDev};
use protect::intent::{IntentMap, MapKind, MemIntentStore};
use protect::parity::geometry::PageRole;
use protect::parity::{Format, Geometry, Health};
use protect::set::{ParityIntent, ParityMember, ParitySet};

pub const FMT: Format = Format {
    ppl_magic: *b"TEST-PPL",
    state_magic: *b"TEST-PST",
    suspicious: |_| false,
};
pub const PMAGIC: [u8; 8] = *b"TESTPAWM";
pub const PSET: [u8; 16] = [7; 16];
/// Member pages: 64 blocks for four members (256 rows and parity pages),
/// 51 for five.
pub const PAGES: u64 = 764;
/// Blocks a region.
pub const RB: u64 = 8;

pub fn uuid(i: usize) -> [u8; 16] {
    [u8::try_from(i).unwrap() + 1; 16]
}

/// A device that logs its reads and can be made to fail all I/O.
#[derive(Clone)]
pub struct Probe {
    pub dev: Arc<MemBlockDev>,
    pub reads: Arc<Mutex<Vec<(u64, u64)>>>,
    pub dead: Arc<AtomicBool>,
}

impl Probe {
    pub fn new(pages: u64) -> Self {
        Self {
            dev: Arc::new(MemBlockDev::new(pages)),
            reads: Arc::default(),
            dead: Arc::default(),
        }
    }
    pub fn kill(&self, on: bool) {
        self.dead.store(on, Ordering::SeqCst);
    }
    pub fn take_reads(&self) -> Vec<(u64, u64)> {
        std::mem::take(&mut self.reads.lock().unwrap())
    }
    pub fn boxed(&self) -> Box<dyn BlockDev> {
        Box::new(self.clone())
    }
    fn check(&self) -> blockdev::Result<()> {
        if self.dead.load(Ordering::SeqCst) {
            return Err(BlockError::Injected {
                kind: "dead",
                write: 0,
            });
        }
        Ok(())
    }
}

impl BlockDev for Probe {
    fn block_size(&self) -> u32 {
        self.dev.block_size()
    }
    fn blocks(&self) -> u64 {
        self.dev.blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        self.check()?;
        self.reads
            .lock()
            .unwrap()
            .push((lba, (buf.len() / protect::PAGE_SIZE) as u64));
        self.dev.read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        self.check()?;
        self.dev.write(lba, buf)
    }
    fn flush(&self) -> blockdev::Result<()> {
        self.check()?;
        self.dev.flush()
    }
}

pub fn probes(n: usize) -> Vec<Probe> {
    probes_of(n, PAGES)
}

pub fn probes_of(n: usize, pages: u64) -> Vec<Probe> {
    (0..n).map(|_| Probe::new(pages)).collect()
}

pub fn stores(n: usize) -> Vec<MemIntentStore> {
    (0..n).map(|_| MemIntentStore::new()).collect()
}

pub fn pintent(stores: &[MemIntentStore]) -> ParityIntent {
    let mut i = ParityIntent::new(PMAGIC);
    i.region_blocks = RB;
    i.away = stores
        .iter()
        .map(|s| Some(Box::new(s.clone()) as _))
        .collect();
    i
}

/// The members as the caller recorded them, each device given if
/// `present`.
pub fn recorded(
    probes: &[Probe],
    health: &[Health],
    present: impl Fn(usize) -> bool,
) -> Vec<ParityMember> {
    probes
        .iter()
        .enumerate()
        .map(|(i, p)| ParityMember {
            uuid: uuid(i),
            health: health[i],
            dev: present(i).then(|| p.boxed()),
        })
        .collect()
}

pub fn healths(set: &ParitySet) -> Vec<Health> {
    set.members().iter().map(|m| m.health).collect()
}

/// A set block in region `r` of member `m`'s rows, `k` into it.
pub fn at(geo: &Geometry, m: u64, r: u64, k: u64) -> u64 {
    let rows = geo.blocks * geo.w() as u64;
    m * rows + r * RB * geo.w() as u64 + k
}

/// The blocks of the layout a list of member reads touched, and whether
/// any was not a row or a parity page.
pub fn blocks_read(geo: &Geometry, member: usize, reads: &[(u64, u64)]) -> (Vec<bool>, bool) {
    let mut seen = vec![false; usize::try_from(geo.blocks).unwrap()];
    let mut other = false;
    for &(lba, count) in reads {
        for p in lba..lba + count {
            let Some(PageRole::Data(s) | PageRole::Parity { stripe: s, .. }) =
                geo.page_of(member, p)
            else {
                other = true;
                continue;
            };
            seen[usize::try_from(geo.block_of(s)).unwrap()] = true;
        }
    }
    (seen, other)
}

/// Run every piece of work to its end; the blocks of the layout the
/// rebuilds read on the members other than the one rebuilt, which must
/// all be in `allowed` regions when given.
pub fn run_all(set: &ParitySet, probes: &[Probe], allowed: Option<&[u64]>) -> u64 {
    let geo = set.geometry();
    for p in probes {
        p.take_reads();
    }
    let mut touched = vec![false; usize::try_from(geo.blocks).unwrap()];
    while let Some(work) = set.work() {
        set.step(3).unwrap();
        if let protect::set::ParityWork::Rebuild(m, _) = work {
            for (i, p) in probes.iter().enumerate() {
                let reads = p.take_reads();
                if i == m {
                    continue;
                }
                let (seen, other) = blocks_read(&geo, i, &reads);
                assert!(!other, "a rebuild read only rows and parity");
                for (b, s) in seen.iter().enumerate() {
                    touched[b] |= s;
                }
            }
        }
    }
    if let Some(allowed) = allowed {
        for (b, &t) in touched.iter().enumerate() {
            assert!(
                !t || allowed.contains(&(b as u64 / RB)),
                "block {b} read, regions {allowed:?}"
            );
        }
    }
    touched.iter().filter(|t| **t).count() as u64
}

/// What a store holds: its kind, or `None` when nothing valid.
pub fn kept_kind(s: &MemIntentStore) -> Option<MapKind> {
    IntentMap::decode(&PMAGIC, &s.bytes()).map(|m| m.kind)
}
