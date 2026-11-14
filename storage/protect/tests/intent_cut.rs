// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The write-intent map of a mirrored set over fault-injecting devices:
//! the power cut at every durable step of setting a bit, writing,
//! flushing and clearing. The members agree after the copy, no write a
//! flush acknowledged is lost, and only dirty regions are read.
//!
//! Two cuts: the devices fail from then on and the set sees it (and
//! takes a member out, which the caller records), or the process dies
//! where it stands and the caller's record is what it was.

mod common;

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use blockdev::{BlockDev, BlockError, MemBlockDev};
use protect::UnitState;
use protect::intent::{IntentMap, IntentStore, MemIntentStore};
use protect::mirror::RESUME_CHUNK;
use protect::set::{Intent, MemJournal, MirrorSet};

use common::{MAGIC, REGION, SET, block, intent, member, pair, resume_all, resync_all, value};

/// What happens when the count runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    /// Every later write and flush fails.
    Fail,
    /// The process dies inside the call.
    Die,
}

const CUT: &str = "power cut";

/// A device under a shared count of durable steps. Write-through, it
/// has no cache: a write is durable when it returns.
struct Cut {
    dev: Arc<MemBlockDev>,
    left: Arc<AtomicI64>,
    through: bool,
    kind: Kind,
}

/// Take a step; false when the power is gone.
fn step(left: &AtomicI64, kind: Kind) -> bool {
    let ok = left.fetch_sub(1, Ordering::SeqCst) > 0;
    assert!(ok || kind == Kind::Fail, "{CUT}");
    ok
}

fn cut_error() -> BlockError {
    BlockError::Injected {
        kind: CUT,
        write: 0,
    }
}

impl BlockDev for Cut {
    fn block_size(&self) -> u32 {
        self.dev.block_size()
    }
    fn blocks(&self) -> u64 {
        self.dev.blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        self.dev.read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        if !step(&self.left, self.kind) {
            return Err(cut_error());
        }
        self.dev.write(lba, buf)?;
        if self.through {
            self.dev.flush()?;
        }
        Ok(())
    }
    fn flush(&self) -> blockdev::Result<()> {
        if !step(&self.left, self.kind) {
            return Err(cut_error());
        }
        self.dev.flush()
    }
}

/// The map's store under the same count.
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

enum Op {
    Write(u64, u32),
    Flush,
}

/// Writes into clean and dirty regions, across a boundary, flushes
/// that clear, a region written again after its bit was cleared.
fn scenario() -> Vec<Op> {
    use Op::{Flush, Write};
    vec![
        Write(0, 1),
        Write(1, 2),
        Write(REGION + 3, 3),
        Flush,
        Write(REGION - 1, 4),
        Write(REGION, 5),
        Write(5 * REGION + 7, 6),
        Flush,
        Write(0, 7),
        Write(3 * REGION, 8),
        Flush,
        Write(3 * REGION, 9),
        Write(5 * REGION + 7, 10),
        Write(7 * REGION + 1, 11),
        Write(1, 12),
        Flush,
        Write(2 * REGION, 13),
    ]
}

/// What a block may hold after a power cut: what the last flush that
/// succeeded made durable, or any value written to it since.
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
    fn allowed(&self, lba: u64, got: u32) -> bool {
        got == self.durable.get(&lba).copied().unwrap_or(0)
            || self.since.get(&lba).is_some_and(|s| s.contains(&got))
    }
}

/// What a run left: the model, whether the cut came, and the members'
/// states as the caller recorded them.
struct Cutoff {
    model: Model,
    cut: bool,
    states: [UnitState; 2],
}

/// Run the scenario with `budget` durable steps before the cut.
fn run_cut(
    a: &Arc<MemBlockDev>,
    b: &Arc<MemBlockDev>,
    store: &MemIntentStore,
    budget: i64,
    through: bool,
    kind: Kind,
) -> Cutoff {
    let left = Arc::new(AtomicI64::new(budget));
    let cut = |d: &Arc<MemBlockDev>| -> Box<dyn BlockDev> {
        Box::new(Cut {
            dev: Arc::clone(d),
            left: Arc::clone(&left),
            through,
            kind,
        })
    };
    let mut i = Intent::new(
        MAGIC,
        Box::new(CutStore {
            store: store.clone(),
            left: Arc::clone(&left),
            kind,
        }),
    );
    i.region_blocks = REGION;
    i.clear_delay = Duration::ZERO;
    i.fresh = true;
    let mut out = Cutoff {
        model: Model::default(),
        cut: true,
        states: [UnitState::Active; 2],
    };
    let opened = catch_unwind(AssertUnwindSafe(|| {
        MirrorSet::open_with(
            SET,
            a.blocks(),
            vec![
                member(cut(a), 1, UnitState::Active),
                member(cut(b), 2, UnitState::Active),
            ],
            Some(i),
        )
    }));
    let Ok(Ok(set)) = opened else {
        return out;
    };
    // What the caller records from the set's events; a process that
    // died recorded nothing more.
    let record = |out: &mut Cutoff| {
        for (i, m) in set.members().iter().enumerate() {
            out.states[i] = m.state;
        }
    };
    for op in scenario() {
        match op {
            Op::Write(lba, v) => match catch_unwind(AssertUnwindSafe(|| set.write(lba, &block(v))))
            {
                Ok(Ok(())) => out.model.wrote(lba, v, true),
                Ok(Err(_)) => {
                    out.model.wrote(lba, v, false);
                    record(&mut out);
                    return out;
                }
                Err(_) => {
                    out.model.wrote(lba, v, false);
                    return out;
                }
            },
            Op::Flush => match catch_unwind(AssertUnwindSafe(|| set.flush())) {
                // A flush that failed on one member and went on is not
                // taken as acknowledged.
                Ok(Ok(())) if left.load(Ordering::SeqCst) >= 0 => out.model.flushed(),
                Ok(_) => {
                    record(&mut out);
                    return out;
                }
                Err(_) => return out,
            },
        }
    }
    record(&mut out);
    out.cut = left.load(Ordering::SeqCst) < 0;
    out
}

#[derive(Debug, Default)]
struct Seen {
    apart: usize,
    copies: usize,
    resumes: usize,
}

#[test]
fn a_power_cut_at_every_step_loses_nothing_a_flush_acknowledged() {
    // A death at the cut is expected: say nothing of it.
    let quiet = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.payload().downcast_ref::<String>().map(String::as_str) != Some(CUT) {
            quiet(info);
        }
    }));
    let blocks = 8 * REGION;
    let steps = {
        let mut n = 1;
        loop {
            let (a, b) = pair(blocks);
            if !run_cut(&a, &b, &MemIntentStore::new(), n, false, Kind::Fail).cut {
                break n;
            }
            n += 1;
        }
    };
    assert!(steps > 40, "{steps} steps");
    let written: Vec<u64> = scenario()
        .iter()
        .filter_map(|o| match o {
            Op::Write(lba, _) => Some(*lba),
            Op::Flush => None,
        })
        .collect();
    let mut seen: HashMap<Kind, Seen> = HashMap::new();
    for kind in [Kind::Fail, Kind::Die] {
        for (budget, through) in (0..=steps).flat_map(|n| [(n, false), (n, true)]) {
            let at = format!("{kind:?} at step {budget}, write-through {through}");
            let (a, b) = pair(blocks);
            let store = MemIntentStore::new();
            let Cutoff { model, states, .. } = run_cut(&a, &b, &store, budget, through, kind);
            // The power goes: what the caches held is gone.
            a.drop_unflushed();
            b.drop_unflushed();
            if IntentMap::decode(&MAGIC, &store.bytes()).is_none() {
                // Cut before the new set's map was written: no set yet.
                continue;
            }
            let set = MirrorSet::open_with(
                SET,
                blocks,
                vec![
                    member(Box::new(Arc::clone(&a)), 1, states[0]),
                    member(Box::new(Arc::clone(&b)), 2, states[1]),
                ],
                Some(intent(&store, false, REGION, Duration::ZERO)),
            )
            .unwrap();
            let s = seen.entry(kind).or_default();
            s.apart += usize::from(a.flushed_image() != b.flushed_image());
            let reads = a.stats().reads;
            let copied = if set.resync_progress().is_some() {
                resync_all(&set)
            } else {
                0
            };
            s.copies += usize::from(copied > 0);
            assert_eq!(a.stats().reads - reads, copied, "{at}: one read per chunk");
            assert!(copied < blocks / RESUME_CHUNK, "{at}: not the whole set");
            // A member the set took out at the cut comes back by a resume.
            for (i, m) in set.members().iter().enumerate() {
                if m.state != UnitState::Active {
                    s.resumes += 1;
                    resume_all(&set, i, &mut MemJournal::default());
                }
            }
            set.flush().unwrap();
            assert!(
                a.flushed_image() == b.flushed_image(),
                "{at}: the members differ"
            );
            for &lba in &written {
                let got = value(&set, lba);
                assert!(
                    model.allowed(lba, got),
                    "{at}: block {lba} holds {got}, durable {:?}, since {:?}",
                    model.durable.get(&lba),
                    model.since.get(&lba)
                );
            }
        }
    }
    let _ = std::panic::take_hook();
    eprintln!("{steps} steps: {seen:?}");
    // The cuts did leave members apart; copies mended the deaths (both
    // members recorded active) and resumes the failures.
    let die = &seen[&Kind::Die];
    assert!(
        die.apart > 10 && die.copies > 10 && die.resumes == 0,
        "{die:?}"
    );
    assert!(seen[&Kind::Fail].resumes > 0);
}
