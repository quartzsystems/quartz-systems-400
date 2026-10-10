// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ASP format and the allocator: format, reopen, random allocation
//! against a model, torn and dropped writes during checkpoints.

use std::collections::BTreeSet;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::alloc::Extent;
use store::{Asp, FormatParams, StoreError};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WAL: u64 = 32;

fn params(units: usize) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xA5; 16],
        unit_uuids: (0..units)
            .map(|i| [u8::try_from(i + 1).unwrap(); 16])
            .collect(),
        wal_pages: WAL,
        now: 1_767_225_600,
    }
}

fn devices(sizes: &[u64], seed: u64) -> Vec<Arc<MemBlockDev>> {
    sizes
        .iter()
        .enumerate()
        .map(|(i, &s)| Arc::new(MemBlockDev::with_seed(s, seed + i as u64)))
        .collect()
}

fn boxed(devs: &[Arc<MemBlockDev>]) -> Vec<Box<dyn BlockDev>> {
    devs.iter()
        .map(|d| Box::new(d.clone()) as Box<dyn BlockDev>)
        .collect()
}

/// Free pages of a fresh ASP before its own metadata: everything but
/// the reserved pages and the WAL.
fn fresh_free(sizes: &[u64]) -> u64 {
    sizes.iter().map(|s| s - 4).sum::<u64>() - WAL
}

#[test]
fn format_and_reopen() {
    let sizes = [2048, 1024];
    let devs = devices(&sizes, 1);
    let asp = Asp::format(boxed(&devs), &params(2)).unwrap();
    let meta = asp.metadata_pages();
    assert!(meta >= 2, "a tree leaf and a log page at least");
    assert_eq!(asp.total_free() + meta, fresh_free(&sizes));
    assert_eq!(asp.superblock().generation, 1);
    assert_eq!(asp.superblock().units.len(), 2);
    let stats = asp.unit_stats();
    assert_eq!(stats[0].free + stats[1].free, asp.total_free());
    assert_eq!(stats[1].pages, 1024);
    let free = asp.total_free();
    drop(asp);
    // Reopen with the units in the other order.
    let mut rev = devs.clone();
    rev.reverse();
    let asp = Asp::open(boxed(&rev)).unwrap();
    assert_eq!(asp.total_free(), free);
    assert_eq!(asp.layout().unit_count(), 2);
    assert_eq!(asp.layout().span(1).start, 2048);
}

#[test]
fn open_refuses_a_different_release_and_a_blank_disk() {
    let devs = devices(&[1024], 3);
    let _ = Asp::format(boxed(&devs), &params(1)).unwrap();
    // Patch both superblock copies' release to V2R0M0.
    for slot in 1..=2u64 {
        let mut buf = vec![0u8; 4096];
        devs[0].read(slot, &mut buf).unwrap();
        if &buf[..8] != b"QS400SUP" {
            continue;
        }
        buf[44] = 2;
        // Recompute the CRC the way the page layer does.
        let mut page = store::Page::from_bytes(&buf);
        let t = page.trailer();
        page.seal(store::PageType::Superblock, t.owner, t.generation);
        devs[0].write(slot, page.bytes()).unwrap();
    }
    devs[0].flush().unwrap();
    assert!(matches!(
        Asp::open(boxed(&devs)),
        Err(StoreError::ReleaseMismatch { .. })
    ));
    let blank = devices(&[1024], 4);
    assert!(matches!(
        Asp::open(boxed(&blank)),
        Err(StoreError::Checksum { .. })
    ));
}

/// The pages a run covers, in logical numbers.
fn pages_of(start: u64, len: u64) -> impl Iterator<Item = u64> {
    start..start + len
}

/// Free pages per the allocator, as logical page numbers.
fn free_set(asp: &Asp) -> BTreeSet<u64> {
    asp.allocator()
        .free_extents()
        .iter()
        .flat_map(|e: &Extent| {
            let start = asp.layout().logical(e.unit, e.lba);
            pages_of(start, e.len)
        })
        .collect()
}

#[test]
fn random_allocations_never_overlap_and_reconcile() {
    let sizes = [4096, 1500, 700];
    let devs = devices(&sizes, 7);
    let mut asp = Asp::format(boxed(&devs), &params(3)).unwrap();
    let total = fresh_free(&sizes);
    let mut rng = Rng(99);
    // Model: allocated runs and runs pending a checkpoint.
    let mut allocated: Vec<(u64, u64)> = Vec::new();
    let mut held: BTreeSet<u64> = BTreeSet::new();
    let mut pending: Vec<(u64, u64)> = Vec::new();
    for step in 0..6000 {
        match rng.below(100) {
            0..=49 => {
                let want = 1 + rng.below(40);
                match asp.alloc(want) {
                    Ok(run) => {
                        assert!(run.len >= 1 && run.len <= want);
                        for p in pages_of(run.start, run.len) {
                            assert!(held.insert(p), "page {p} handed out twice at step {step}");
                        }
                        allocated.push((run.start, run.len));
                    }
                    Err(StoreError::NoSpace { .. }) => assert_eq!(asp.available(), 0),
                    Err(e) => panic!("{e}"),
                }
            }
            50..=54 => {
                if let Some(&(s, l)) = allocated.last() {
                    let got = asp.extend(s + l, 1 + rng.below(8));
                    for p in pages_of(s + l, got) {
                        assert!(held.insert(p), "extend handed out {p} twice");
                    }
                    let n = allocated.len();
                    allocated[n - 1].1 += got;
                }
            }
            55..=89 => {
                if !allocated.is_empty() {
                    let i = usize::try_from(rng.below(allocated.len() as u64)).unwrap();
                    let (s, l) = allocated.swap_remove(i);
                    asp.free(s, l).unwrap();
                    assert!(
                        matches!(asp.free(s, l), Err(StoreError::Corrupt { .. })),
                        "double free"
                    );
                    pending.push((s, l));
                }
            }
            90..=95 => {
                asp.checkpoint().unwrap();
                for (s, l) in pending.drain(..) {
                    for p in pages_of(s, l) {
                        held.remove(&p);
                    }
                }
            }
            _ => {
                // Reopen from disk: the free set must match the last
                // checkpoint plus nothing else.
                asp.checkpoint().unwrap();
                for (s, l) in pending.drain(..) {
                    for p in pages_of(s, l) {
                        held.remove(&p);
                    }
                }
                let before = free_set(&asp);
                drop(asp);
                asp = Asp::open(boxed(&devs)).unwrap();
                assert_eq!(
                    free_set(&asp),
                    before,
                    "free set after reopen at step {step}"
                );
            }
        }
        // Reconcile: free + held (allocated and pending) + the
        // allocator's own pages == total.
        let meta = asp.metadata_pages();
        assert_eq!(
            asp.total_free() + held.len() as u64 + meta,
            total,
            "step {step}: free {} held {} meta {meta}",
            asp.total_free(),
            held.len()
        );
        let free = free_set(&asp);
        assert!(
            free.is_disjoint(&held),
            "a held page is free at step {step}"
        );
    }
}

#[test]
fn fragmented_requests_get_shorter_runs() {
    let devs = devices(&[512], 11);
    let mut asp = Asp::format(boxed(&devs), &params(1)).unwrap();
    let mut runs = Vec::new();
    loop {
        match asp.alloc(2) {
            Ok(r) => runs.push(r),
            Err(StoreError::NoSpace { .. }) => break,
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(asp.available(), 0);
    assert_eq!(
        asp.total_free(),
        asp.allocator().reserve(),
        "the reserve is untouched"
    );
    // Free every other run, checkpoint, and ask for a big one: the
    // allocator returns the largest run it has, shorter than asked.
    for r in runs.iter().step_by(2) {
        asp.free(r.start, r.len).unwrap();
    }
    asp.checkpoint().unwrap();
    let big = asp.alloc(100).unwrap();
    assert!(big.len >= 2 && big.len < 100, "got {}", big.len);
    assert!(
        matches!(asp.free(0, 1), Err(StoreError::Corrupt { .. })),
        "reserved page"
    );
    assert!(
        matches!(asp.free(511, 1), Err(StoreError::Corrupt { .. })),
        "backup header"
    );
}

/// Run `script` on a fresh ASP and count the writes its checkpoint
/// makes; return that and the free set after the checkpoint.
fn checkpoint_outcome(sizes: &[u64], script: &dyn Fn(&mut Asp)) -> (u64, BTreeSet<u64>) {
    let devs = devices(sizes, 21);
    let mut asp = Asp::format(boxed(&devs), &params(sizes.len())).unwrap();
    script(&mut asp);
    let before: u64 = devs.iter().map(|d| d.stats().writes).sum();
    asp.checkpoint().unwrap();
    let after: u64 = devs.iter().map(|d| d.stats().writes).sum();
    (after - before, free_set(&asp))
}

#[test]
fn torn_or_dropped_writes_during_checkpoint_keep_a_valid_state() {
    let sizes = [1024, 512];
    let script = |asp: &mut Asp| {
        let mut runs = Vec::new();
        for _ in 0..200 {
            runs.push(asp.alloc(3).unwrap());
        }
        for r in runs.iter().step_by(3) {
            asp.free(r.start, r.len).unwrap();
        }
    };
    let (n, after) = checkpoint_outcome(&sizes, &script);
    assert!(
        n >= 4,
        "a checkpoint writes the tree, the log and the superblocks"
    );
    for k in 1..=n {
        let devs = devices(&sizes, 21);
        let mut asp = Asp::format(boxed(&devs), &params(2)).unwrap();
        let before = free_set(&asp);
        script(&mut asp);
        // The k-th write of the checkpoint is torn and every later one
        // fails, so the flush never comes unless the torn write was the
        // last. Then the power goes.
        for d in &devs {
            let w = d.stats().writes;
            d.tear_at(Some(w + k));
            d.fail_after(Some(w + k));
        }
        let _ = asp.checkpoint();
        drop(asp);
        for d in &devs {
            d.drop_unflushed();
            d.tear_at(None);
            d.fail_after(None);
        }
        let asp = Asp::open(boxed(&devs))
            .unwrap_or_else(|e| panic!("reopen after tear at write {k}: {e}"));
        match asp.superblock().generation {
            1 => assert_eq!(
                free_set(&asp),
                before,
                "tear at write {k}: generation 1 state"
            ),
            2 => assert_eq!(
                free_set(&asp),
                after,
                "tear at write {k}: generation 2 state"
            ),
            g => panic!("generation {g}"),
        }
    }
}

#[test]
fn torn_superblock_falls_back_to_the_other_copy() {
    let sizes = [1024, 512];
    let devs = devices(&sizes, 31);
    let mut asp = Asp::format(boxed(&devs), &params(2)).unwrap();
    for _ in 0..50 {
        asp.alloc(2).unwrap();
    }
    asp.checkpoint().unwrap();
    let after_two = free_set(&asp);
    assert_eq!(asp.superblock().generation, 2);
    for _ in 0..50 {
        asp.alloc(2).unwrap();
    }
    asp.checkpoint().unwrap();
    assert_eq!(asp.superblock().generation, 3);
    drop(asp);
    // Generation 3 lives in slot 1 (page 2) on both units. Damage it
    // there: the newest valid copy is now generation 2 in slot 0.
    for d in &devs {
        let mut buf = vec![0u8; 4096];
        d.read(2, &mut buf).unwrap();
        assert_eq!(&buf[..8], b"QS400SUP");
        buf[100] ^= 1;
        d.write(2, &buf).unwrap();
        d.flush().unwrap();
    }
    let asp = Asp::open(boxed(&devs)).unwrap();
    assert_eq!(asp.superblock().generation, 2);
    assert_eq!(free_set(&asp), after_two);
}
