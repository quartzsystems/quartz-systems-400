// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parity set's maps of members out of service over fault-injecting
//! devices: the power cut at every durable step of a member failing in
//! the middle of a flush, writes while it is away, its return and its
//! rebuild from the map. After each cut every block holds what the last
//! acknowledged flush made durable or a value written since, every
//! stripe's parity is in step, and a rebuild from a map read only the
//! blocks of its regions.
//!
//! Two cuts: the devices and stores fail from then on and the set sees
//! it (the caller records what it reports), or the process dies where
//! it stands and the caller's record is what it was.

mod common;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use blockdev::{BlockDev, BlockError};
use protect::intent::{IntentStore, MemIntentStore};
use protect::parity::{Geometry, Health, Level};
use protect::set::{ParityIntent, ParitySet, ParityWork};

use common::parity::{
    FMT, PMAGIC, PSET, Probe, RB, at, healths, probes_of, recorded, run_all, stores, uuid,
};
use common::{block, value};

/// Member pages: 128 blocks for four members (16 regions), 102 for five
/// (13), so that a map leaves much of the set out.
const PAGES: u64 = 1020;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    /// Every later write, flush and store write fails.
    Fail,
    /// The process dies inside the call.
    Die,
}

const CUT: &str = "power cut";

fn step(left: &AtomicI64, kind: Kind) -> bool {
    let ok = left.fetch_sub(1, Ordering::SeqCst) > 0;
    assert!(ok || kind == Kind::Fail, "{CUT}");
    ok
}

/// A member under the shared count of durable steps; I/O to a member
/// killed by the scenario fails without taking one.
struct Cut {
    p: Probe,
    left: Arc<AtomicI64>,
    through: bool,
    kind: Kind,
}

impl BlockDev for Cut {
    fn block_size(&self) -> u32 {
        self.p.block_size()
    }
    fn blocks(&self) -> u64 {
        self.p.blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        self.p.read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        if self.p.dead.load(Ordering::SeqCst) {
            return self.p.write(lba, buf);
        }
        if !step(&self.left, self.kind) {
            return Err(BlockError::Injected {
                kind: CUT,
                write: 0,
            });
        }
        self.p.write(lba, buf)?;
        if self.through {
            self.p.flush()?;
        }
        Ok(())
    }
    fn flush(&self) -> blockdev::Result<()> {
        if self.p.dead.load(Ordering::SeqCst) {
            return self.p.flush();
        }
        if !step(&self.left, self.kind) {
            return Err(BlockError::Injected {
                kind: CUT,
                write: 0,
            });
        }
        self.p.flush()
    }
}

/// A member's map store under the same count.
struct CutStore {
    store: MemIntentStore,
    left: Arc<AtomicI64>,
    kind: Kind,
}

impl IntentStore for CutStore {
    fn read_all(&mut self) -> std::io::Result<Vec<u8>> {
        self.store.read_all()
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        if !step(&self.left, self.kind) {
            return Err(std::io::Error::other(CUT));
        }
        self.store.write(offset, bytes)
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Write(u64, u32),
    Flush,
    /// The member's device fails all I/O from now on.
    Kill(usize),
    /// The set is stopped without a flush and opened again as the caller
    /// recorded it, every member's device back.
    Reopen,
    /// One step of the rebuild.
    Step,
    /// The rebuild to its end.
    Rebuild,
}

/// Writes, member 1 failing with writes held for the flush (RAID 6:
/// member 3 too, later), writes while away into new regions and its own
/// rows, the return, writes during the rebuild into regions of the map
/// and outside it, writes after.
fn scenario(geo: &Geometry, raid6: bool) -> Vec<Op> {
    use Op::{Flush, Kill, Rebuild, Reopen, Step, Write};
    let a = |m, r, k| at(geo, m, r, k);
    let mut ops = vec![
        Write(a(0, 0, 0), 1),
        Write(a(2, 1, 5), 2),
        Flush,
        Write(a(0, 3, 1), 3),
        Write(a(1, 6, 2), 4),
        Kill(1),
        Flush,
        Write(a(3, 4, 0), 5),
        Write(a(1, 2, 7), 6),
        Flush,
    ];
    if raid6 {
        ops.extend([Kill(3), Write(a(3, 2, 0), 13)]);
    }
    ops.extend([
        Write(a(0, 4, 1), 7),
        Write(a(2, 6, 3), 8),
        Flush,
        Write(a(3, 0, 9), 9),
        Reopen,
        Step,
        Write(a(1, 3, 0), 10),
        Flush,
        Step,
        Write(a(0, 5, 2), 11),
        Rebuild,
        Write(a(1, 1, 1), 12),
        Flush,
    ]);
    ops
}

#[derive(Default)]
struct Model {
    durable: HashMap<u64, u32>,
    since: HashMap<u64, Vec<u32>>,
    unflushed: HashMap<u64, u32>,
}

impl Model {
    fn wrote(&mut self, lba: u64, v: u32, ok: bool) {
        self.since.entry(lba).or_default().push(v);
        if ok {
            self.unflushed.insert(lba, v);
        }
    }
    fn flushed(&mut self) {
        for (lba, v) in self.unflushed.drain() {
            self.durable.insert(lba, v);
        }
        self.since.clear();
    }
    /// A reopen drops what the set held for a flush.
    fn dropped(&mut self) {
        self.unflushed.clear();
    }
    fn allowed(&self, lba: u64, got: u32) -> bool {
        got == self.durable.get(&lba).copied().unwrap_or(0)
            || self.since.get(&lba).is_some_and(|s| s.contains(&got))
    }
}

struct Run {
    model: Model,
    cut: bool,
    record: Vec<Health>,
    probes: Vec<Probe>,
    stores: Vec<MemIntentStore>,
}

fn level(n: usize) -> Level {
    if n == 4 { Level::Raid5 } else { Level::Raid6 }
}

/// Run the scenario with `budget` durable steps after the set is made.
#[allow(clippy::too_many_lines)] // one scenario, read top to bottom
fn run_cut(n: usize, budget: i64, through: bool, kind: Kind) -> Run {
    let p = probes_of(n, PAGES);
    let st = stores(n);
    let left = Arc::new(AtomicI64::new(i64::MAX));
    let devs = |p: &[Probe]| -> Vec<Box<dyn BlockDev>> {
        p.iter()
            .map(|p| {
                Box::new(Cut {
                    p: p.clone(),
                    left: Arc::clone(&left),
                    through,
                    kind,
                }) as Box<dyn BlockDev>
            })
            .collect()
    };
    let intent = || {
        let mut i = ParityIntent::new(PMAGIC);
        i.region_blocks = RB;
        i.away = st
            .iter()
            .map(|s| {
                Some(Box::new(CutStore {
                    store: s.clone(),
                    left: Arc::clone(&left),
                    kind,
                }) as Box<dyn IntentStore>)
            })
            .collect();
        i
    };
    let set = ParitySet::create_with(
        FMT,
        PSET,
        level(n),
        0,
        devs(&p)
            .into_iter()
            .enumerate()
            .map(|(i, d)| (uuid(i), d))
            .collect(),
        Some(intent()),
    )
    .unwrap();
    while set.step(64).unwrap().is_some() {}
    let geo = set.geometry();
    let mut out = Run {
        model: Model::default(),
        cut: true,
        record: healths(&set),
        probes: p.clone(),
        stores: st.clone(),
    };
    left.store(budget, Ordering::SeqCst);
    let mut set = Some(set);
    for op in scenario(&geo, n == 5) {
        let s = set.as_ref().expect("a set");
        let r = catch_unwind(AssertUnwindSafe(|| -> Result<bool, ()> {
            match op {
                Op::Write(lba, v) => {
                    // A write that died part way may have landed.
                    out.model.wrote(lba, v, false);
                    let ok = s.write(lba, &block(v)).is_ok();
                    if ok {
                        out.model.unflushed.insert(lba, v);
                    }
                    Ok(ok)
                }
                Op::Flush => {
                    let ok = s.flush().is_ok() && left.load(Ordering::SeqCst) >= 0;
                    if ok {
                        out.model.flushed();
                    }
                    Ok(ok)
                }
                Op::Kill(m) => {
                    p[m].kill(true);
                    Ok(true)
                }
                Op::Step => Ok(s.step(4).is_ok()),
                Op::Rebuild => loop {
                    match s.step(4) {
                        Ok(Some(pr)) if !pr.complete => {}
                        Ok(_) => break Ok(true),
                        Err(_) => break Ok(false),
                    }
                },
                Op::Reopen => Ok(true),
            }
        }));
        match r {
            Ok(Ok(true)) => out.record = healths(s),
            Ok(_) => {
                out.record = healths(s);
                return out;
            }
            // Died where it stood: the caller recorded nothing more.
            Err(_) => return out,
        }
        if let Op::Reopen = op {
            drop(set.take());
            out.model.dropped();
            for q in &p {
                q.kill(false);
            }
            let members = recorded(&p, &out.record, |_| true)
                .into_iter()
                .zip(devs(&p))
                .map(|(mut m, d)| {
                    m.dev = Some(d);
                    m
                })
                .collect();
            let opened = catch_unwind(AssertUnwindSafe(|| {
                ParitySet::open_with(FMT, PSET, geo, members, None, Some(intent()))
            }));
            match opened {
                Ok(Ok(s)) => {
                    out.record = healths(&s);
                    set = Some(s);
                }
                _ => return out,
            }
        }
    }
    out.cut = left.load(Ordering::SeqCst) < 0;
    out
}

#[derive(Debug, Default)]
struct Seen {
    runs: usize,
    /// Rebuilds after the cut from a map, and from none.
    mapped: usize,
    whole: usize,
    /// Blocks the mapped rebuilds read, and the set's blocks for each.
    read: u64,
    of: u64,
}

fn cut_everywhere(n: usize) -> (i64, HashMap<Kind, Seen>) {
    let steps = {
        let mut k = 1;
        while run_cut(n, k, false, Kind::Fail).cut {
            k += 1;
        }
        k
    };
    let mut seen: HashMap<Kind, Seen> = HashMap::new();
    for kind in [Kind::Fail, Kind::Die] {
        for (budget, through) in (0..=steps).flat_map(|k| [(k, false), (k, true)]) {
            let here = format!(
                "{:?}: {kind:?} at step {budget}, write-through {through}",
                level(n)
            );
            let Run {
                model,
                record,
                probes,
                stores,
                ..
            } = run_cut(n, budget, through, kind);
            // The power goes: what the caches held is gone.
            for p in &probes {
                p.dev.drop_unflushed();
                p.kill(false);
            }
            let geo = ParitySet::plan(level(n), n, PAGES, 0).unwrap();
            let mut intent = ParityIntent::new(PMAGIC);
            intent.region_blocks = RB;
            intent.away = stores
                .iter()
                .map(|s| Some(Box::new(s.clone()) as Box<dyn IntentStore>))
                .collect();
            let set = ParitySet::open_with(
                FMT,
                PSET,
                geo,
                recorded(&probes, &record, |_| true),
                None,
                Some(intent),
            )
            .unwrap_or_else(|e| panic!("{here}: {e}"));
            let s = seen.entry(kind).or_default();
            s.runs += 1;
            // Every member being rebuilt from a map: only its regions.
            let mut allowed = Vec::new();
            let mut whole = false;
            let mut rebuilding = false;
            for (i, m) in set.members().iter().enumerate() {
                if matches!(m.health, Health::Rebuilding(_)) {
                    rebuilding = true;
                    match set.away_regions(i) {
                        Some(r) => allowed.extend(r),
                        None => whole = true,
                    }
                }
            }
            if rebuilding {
                assert!(matches!(set.work(), Some(ParityWork::Rebuild(..))));
            }
            let read = run_all(&set, &probes, (!whole).then_some(allowed.as_slice()));
            if rebuilding && whole {
                s.whole += 1;
            } else if rebuilding {
                s.mapped += 1;
                s.read += read;
                s.of += geo.blocks;
                assert!(read < geo.blocks, "{here}: not the whole set");
            }
            assert!(
                set.members().iter().all(|m| m.health == Health::Ok),
                "{here}: {:?}",
                set.members()
            );
            assert_eq!(set.verify().unwrap(), Vec::<u64>::new(), "{here}: parity");
            for op in scenario(&geo, n == 5) {
                if let Op::Write(lba, _) = op {
                    let got = value(&set, lba);
                    assert!(
                        model.allowed(lba, got),
                        "{here}: block {lba} holds {got}, durable {:?}, since {:?}",
                        model.durable.get(&lba),
                        model.since.get(&lba)
                    );
                }
            }
        }
    }
    (steps, seen)
}

fn quiet() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<String>().map(String::as_str) != Some(CUT) {
            prev(info);
        }
    }));
}

#[test]
fn a_power_cut_at_every_step_of_a_raid5_member_away_and_back() {
    quiet();
    let (steps, seen) = cut_everywhere(4);
    eprintln!("RAID 5: {steps} steps: {seen:?}");
    assert!(steps > 60, "{steps} steps");
    for s in seen.values() {
        assert!(s.mapped > 20, "{s:?}");
    }
}

#[test]
fn a_power_cut_at_every_step_of_two_raid6_members_away_and_back() {
    quiet();
    let (steps, seen) = cut_everywhere(5);
    eprintln!("RAID 6: {steps} steps: {seen:?}");
    assert!(steps > 60, "{steps} steps");
    for s in seen.values() {
        assert!(s.mapped > 20, "{s:?}");
    }
}
