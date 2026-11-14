// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The write-intent map of a mirrored set: only dirty regions copied
//! after an unclean stop, bits set once and cleared in batches, a copy
//! cut short going on, members back from service copied from their own
//! maps. The power cuts are `intent_cut.rs`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use blockdev::{AlignedBuf, BlockDev};
use protect::UnitState;
use protect::intent::{IntentMap, IntentStore, MapKind, MemIntentStore};
use protect::set::{Intent, MemJournal, MirrorSet};

use common::{
    MAGIC, REGION, SET, block, intent, member, open, pair, resume_all, resync_all, value,
};

#[test]
fn an_unclean_stop_copies_only_the_dirty_regions() {
    let blocks = 16 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(&a, &b, blocks, intent(&store, true, REGION, Duration::ZERO));
    assert!(set.resync_progress().is_none());
    for (n, lba) in [3 * REGION, 9 * REGION + 5, 9 * REGION + 6]
        .into_iter()
        .enumerate()
    {
        set.write(lba, &block(u32::try_from(n).unwrap() + 1))
            .unwrap();
    }
    assert_eq!(set.dirty_regions(), [3, 9]);
    set.flush().unwrap();
    assert!(set.dirty_regions().is_empty(), "cleared at the flush");

    // A write in flight at the stop: on the first member only.
    set.write(5 * REGION + 1, &block(50)).unwrap();
    set.flush().unwrap();
    set.write(5 * REGION + 2, &block(51)).unwrap();
    b.drop_unflushed();
    a.flush().unwrap();
    drop(set);
    let kept = IntentMap::decode(&MAGIC, &store.bytes()).unwrap();
    assert_eq!(kept.dirty().collect::<Vec<_>>(), [5]);

    let rb = b.stats().reads;
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    let p = set.resync_progress().unwrap();
    assert_eq!((p.chunks_dirty, p.chunks, p.complete), (2, 32, false));
    // Until the copy, the region reads as the first member has it.
    for _ in 0..4 {
        assert_eq!(
            value(&set, 5 * REGION + 2),
            value(a.as_ref(), 5 * REGION + 2)
        );
    }
    let reads_before = a.stats().reads;
    assert_eq!(resync_all(&set), 2);
    assert_eq!(
        a.stats().reads - reads_before,
        2,
        "the source read twice: two chunks"
    );
    assert_eq!(b.stats().reads, rb, "the target is not read");
    set.flush().unwrap();
    assert_eq!(a.flushed_image(), b.flushed_image());
    assert_eq!(
        value(&set, 5 * REGION + 1),
        50,
        "the flushed write is there"
    );
    assert_eq!(value(&set, 3 * REGION), 1);
    set.close().unwrap();
    assert_eq!(
        IntentMap::decode(&MAGIC, &store.bytes()).unwrap().count(),
        0,
        "a clean stop leaves no region set"
    );
}

#[test]
fn a_region_written_again_and_again_is_set_once() {
    let blocks = 4 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, true, REGION, Duration::from_secs(3600)),
    );
    let first = store.writes();
    for n in 0..100 {
        set.write(REGION + n % 7, &block(n.try_into().unwrap()))
            .unwrap();
        set.flush().unwrap();
    }
    let s = set.intent_stats().unwrap();
    assert_eq!((s.sets, s.clears, s.dirty, s.regions), (1, 0, 1, 4));
    assert_eq!(
        store.writes() - first,
        1,
        "one durable write for 100 writes"
    );

    // A write across a boundary sets both regions in one write.
    set.write(3 * REGION - 1, &AlignedBuf::new(2)).unwrap();
    assert_eq!(set.dirty_regions(), [1, 2, 3]);
    assert_eq!(set.intent_stats().unwrap().sets, 2);
    set.close().unwrap();
    let s = set.intent_stats().unwrap();
    assert_eq!(
        (s.clears, s.dirty),
        (1, 0),
        "every bit cleared in one write"
    );

    // With no delay each flush clears what it made durable.
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    for n in 0..5 {
        set.write(0, &block(n)).unwrap();
        set.flush().unwrap();
    }
    let s = set.intent_stats().unwrap();
    assert_eq!((s.sets, s.clears, s.dirty), (5, 5, 0));
}

#[test]
fn a_write_whose_bit_cannot_be_kept_is_refused() {
    let blocks = 2 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(&a, &b, blocks, intent(&store, true, REGION, Duration::ZERO));
    store.fail(true);
    assert!(set.write(0, &block(1)).is_err());
    assert_eq!(value(a.as_ref(), 0), 0, "nothing reached a member");
    store.fail(false);
    set.write(0, &block(1)).unwrap();
    assert_eq!(value(b.as_ref(), 0), 1);
}

#[test]
fn a_set_with_no_map_kept_copies_every_region() {
    let blocks = 3 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    let p = set.resync_progress().unwrap();
    assert_eq!((p.chunks_dirty, p.chunks), (6, 6));
    assert_eq!(resync_all(&set), 6);
    assert!(
        open(
            &a,
            &b,
            blocks,
            intent(&store, false, 2 * REGION, Duration::ZERO)
        )
        .resync_progress()
        .is_some(),
        "a map of another region size is not this set's"
    );
    assert!(
        MirrorSet::open_with(
            SET,
            blocks,
            vec![
                member(Box::new(Arc::clone(&a)), 1, UnitState::Active),
                member(Box::new(Arc::clone(&b)), 2, UnitState::Active),
            ],
            Some(intent(&store, true, 100, Duration::ZERO)),
        )
        .is_err(),
        "a region is a multiple of the chunk"
    );
}

#[test]
fn a_copy_cut_short_goes_on_after_a_further_power_loss() {
    let blocks = 8 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(&a, &b, blocks, intent(&store, true, REGION, Duration::ZERO));
    for r in [1, 4, 6] {
        set.write(r * REGION + 2, &block(u32::try_from(r).unwrap()))
            .unwrap();
    }
    // The writes reached the first member only.
    b.drop_unflushed();
    a.flush().unwrap();
    drop(set);

    // The copy does one region, then the power goes again.
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    assert_eq!(set.resync_progress().unwrap().chunks_dirty, 6);
    set.resync_step(2).unwrap();
    set.flush().unwrap();
    assert_eq!(set.dirty_regions(), [4, 6], "the region copied is cleared");
    drop(set);
    a.drop_unflushed();
    b.drop_unflushed();

    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    assert_eq!(set.resync_progress().unwrap().chunks_dirty, 4);
    assert_eq!(resync_all(&set), 4);
    set.close().unwrap();
    assert_eq!(a.flushed_image(), b.flushed_image());
    assert!(set.dirty_regions().is_empty());
}

fn away_intent(store: &MemIntentStore, fresh: bool, away: &[MemIntentStore]) -> Intent {
    let mut i = intent(store, fresh, REGION, Duration::ZERO);
    i.away = away
        .iter()
        .map(|s| Some(Box::new(s.clone()) as Box<dyn IntentStore>))
        .collect();
    i
}

#[test]
fn a_member_back_from_service_is_copied_from_its_own_map() {
    let blocks = 8 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let away = [MemIntentStore::new(), MemIntentStore::new()];
    let set = open(&a, &b, blocks, away_intent(&store, true, &away));
    set.write(0, &block(1)).unwrap();
    set.flush().unwrap();

    // Suspended: the regions written while it is away are its map's,
    // though the set's own bits are cleared.
    set.suspend(1).unwrap();
    for r in [2, 5] {
        set.write(r * REGION, &block(10 + u32::try_from(r).unwrap()))
            .unwrap();
    }
    set.flush().unwrap();
    assert!(set.dirty_regions().is_empty());
    let kept = IntentMap::decode(&MAGIC, &away[1].bytes()).unwrap();
    assert_eq!((kept.kind, kept.member), (MapKind::Away, [2; 16]));
    assert_eq!(kept.dirty().collect::<Vec<_>>(), [2, 5]);
    let mut journal = MemJournal::default();
    let writes = b.stats().writes;
    assert_eq!(
        resume_all(&set, 1, &mut journal),
        4,
        "two regions, four chunks"
    );
    assert_eq!(b.stats().writes - writes, 4);
    assert_eq!(a.flushed_image(), b.flushed_image());
    assert_eq!(
        IntentMap::decode(&MAGIC, &away[1].bytes()).map(|m| m.kind),
        Some(MapKind::Off),
        "given up once the member is in step"
    );

    // Pulled, the set stopped while it is away, and the same disk back
    // at the next open.
    let pulled = set.detach(0).unwrap().unwrap();
    set.write(7 * REGION + 9, &block(70)).unwrap();
    set.close().unwrap();
    drop((set, pulled));
    let set = MirrorSet::open_with(
        SET,
        blocks,
        vec![
            member(Box::new(Arc::clone(&a)), 1, UnitState::Missing),
            member(Box::new(Arc::clone(&b)), 2, UnitState::Active),
        ],
        Some(away_intent(&store, false, &away)),
    )
    .unwrap();
    assert_eq!(set.members()[0].state, UnitState::Suspended);
    assert_eq!(resume_all(&set, 0, &mut journal), 2);
    assert_eq!(a.flushed_image(), b.flushed_image());

    // A member that fails keeps a map too; one with no store comes
    // back by a full copy.
    b.fail_after(Some(b.stats().writes));
    set.write(3 * REGION, &block(30)).unwrap();
    assert_eq!(set.members()[1].state, UnitState::NotOperational);
    b.fail_after(None);
    assert_eq!(resume_all(&set, 1, &mut journal), 2);
    set.flush().unwrap();
    assert_eq!(a.flushed_image(), b.flushed_image());
    drop(set);
    let set = open(
        &a,
        &b,
        blocks,
        intent(&store, false, REGION, Duration::ZERO),
    );
    set.suspend(1).unwrap();
    set.write(0, &block(2)).unwrap();
    assert_eq!(resume_all(&set, 1, &mut journal), 16, "every chunk");
}

#[test]
fn a_resume_cut_by_a_stop_copies_again_what_was_written_since() {
    let blocks = 8 * REGION;
    let (a, b) = pair(blocks);
    let store = MemIntentStore::new();
    let set = open(&a, &b, blocks, intent(&store, true, REGION, Duration::ZERO));
    set.suspend(1).unwrap();
    set.write(0, &block(1)).unwrap();
    let mut journal = MemJournal::default();
    let mut r = set.resume(1, &mut journal).unwrap();
    set.resume_step(&mut r, &mut journal, 6).unwrap();
    assert!(journal.saved.as_ref().unwrap().is_done(0));
    // Written while it resumes, on the first member only, and stopped.
    set.write(REGION / 2, &block(5)).unwrap();
    b.write(REGION / 2, &block(66)).unwrap();
    drop((r, set));
    a.drop_unflushed();
    b.drop_unflushed();
    let set = MirrorSet::open_with(
        SET,
        blocks,
        vec![
            member(Box::new(Arc::clone(&a)), 1, UnitState::Active),
            member(Box::new(Arc::clone(&b)), 2, UnitState::Resuming),
        ],
        Some(intent(&store, false, REGION, Duration::ZERO)),
    )
    .unwrap();
    assert!(
        set.resync_progress().is_none(),
        "one active member: nothing between them"
    );
    let mut r = set.resume(1, &mut journal).unwrap();
    assert!(!r.progress().complete);
    while !set.resume_step(&mut r, &mut journal, 4).unwrap().complete {}
    assert_eq!(a.flushed_image(), b.flushed_image());
}
