// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A parity set's maps of members out of service: a member back is
//! rebuilt in its map's regions only, and every block reads as written
//! with every stripe's parity in step.

mod common;

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blockdev::BlockDev;
use protect::intent::{IntentStore, MapKind, MemIntentStore};
use protect::parity::{Health, Level};
use protect::set::{ParityIntent, ParitySet, ParityWork, SetEvent};

use common::parity::{
    FMT, PSET, Probe, at, healths, kept_kind, pintent, probes, recorded, run_all, stores, uuid,
};
use common::{block, value};

/// A new set over `probes`, synchronized.
fn fresh(level: Level, probes: &[Probe], intent: Option<ParityIntent>) -> ParitySet {
    let set = ParitySet::create_with(
        FMT,
        PSET,
        level,
        0,
        probes
            .iter()
            .enumerate()
            .map(|(i, p)| (uuid(i), p.boxed()))
            .collect(),
        intent,
    )
    .unwrap();
    while set.step(64).unwrap().is_some() {}
    set
}

/// Blocks written, and what each holds.
#[derive(Default)]
struct Written(HashMap<u64, u32>);

impl Written {
    fn put(&mut self, set: &ParitySet, lba: u64, v: u32) {
        set.write(lba, &block(v)).unwrap();
        self.0.insert(lba, v);
    }
    fn check(&self, set: &ParitySet) {
        for (&lba, &v) in &self.0 {
            assert_eq!(value(set, lba), v, "block {lba}");
        }
        assert!(set.verify().unwrap().is_empty(), "parity out of step");
    }
}

fn reopen(
    probes: &[Probe],
    health: &[Health],
    stores: &[MemIntentStore],
    present: impl Fn(usize) -> bool,
) -> ParitySet {
    let geo = ParitySet::plan(
        if probes.len() == 4 {
            Level::Raid5
        } else {
            Level::Raid6
        },
        probes.len(),
        common::parity::PAGES,
        0,
    )
    .unwrap();
    ParitySet::open_with(
        FMT,
        PSET,
        geo,
        recorded(probes, health, present),
        None,
        Some(pintent(stores)),
    )
    .unwrap()
}

#[test]
fn a_member_back_is_rebuilt_in_its_regions_only() {
    for (level, n) in [(Level::Raid5, 4), (Level::Raid6, 5)] {
        let p = probes(n);
        let st = stores(n);
        let set = fresh(level, &p, Some(pintent(&st)));
        let geo = set.geometry();
        let mut w = Written::default();
        for m in 0..n as u64 {
            for r in 0..geo.blocks / common::parity::RB {
                w.put(
                    &set,
                    at(&geo, m, r, 1),
                    100 + u32::try_from(m * 10 + r).unwrap(),
                );
            }
        }
        set.flush().unwrap();
        assert_eq!(set.away_regions(1), None);

        // Member 1 leaves; writes go on without it in two regions.
        let dev = set.detach(1).unwrap();
        assert!(dev.is_some(), "the device given back");
        assert_eq!(set.away_regions(1), Some(vec![]));
        assert_eq!(kept_kind(&st[1]), Some(MapKind::Away));
        w.put(&set, at(&geo, 0, 2, 0), 1);
        w.put(&set, at(&geo, 1, 5, 2), 2); // its own row
        w.put(&set, at(&geo, 3, 2, 1), 3);
        set.flush().unwrap();
        assert_eq!(set.away_regions(1), Some(vec![2, 5]));
        w.check(&set);
        let health = healths(&set);
        assert_eq!(health[1], Health::Failed);
        drop(set);

        // It is back: only regions 2 and 5 are read to rebuild it.
        let set = reopen(&p, &health, &st, |_| true);
        assert_eq!(set.drain_events(), [SetEvent::Stale { member: 1 }]);
        assert_eq!(set.work(), Some(ParityWork::Rebuild(1, 0)));
        let read = run_all(&set, &p, Some(&[2, 5]));
        assert_eq!(read, 2 * common::parity::RB, "{level:?}");
        assert!(healths(&set).iter().all(|h| *h == Health::Ok));
        assert_eq!(kept_kind(&st[1]), Some(MapKind::Off), "the map given up");
        assert_eq!(set.away_regions(1), None);
        w.check(&set);
        // Now another member can go and nothing is lost.
        set.detach(0).unwrap();
        w.check(&set);
    }
}

#[test]
fn a_member_failing_in_a_flush_keeps_what_was_in_flight() {
    let p = probes(4);
    let st = stores(4);
    let set = fresh(Level::Raid5, &p, Some(pintent(&st)));
    let geo = set.geometry();
    let mut w = Written::default();
    w.put(&set, at(&geo, 0, 0, 0), 9);
    set.flush().unwrap();
    // Held for the flush when the member dies.
    w.put(&set, at(&geo, 0, 3, 1), 1);
    w.put(&set, at(&geo, 1, 6, 2), 2);
    p[1].kill(true);
    set.flush().unwrap();
    assert!(
        set.drain_events()
            .iter()
            .any(|e| matches!(e, SetEvent::Failed { member: 1, .. }))
    );
    assert_eq!(set.away_regions(1), Some(vec![3, 6]));
    w.put(&set, at(&geo, 2, 4, 0), 3);
    set.flush().unwrap();
    w.check(&set);
    let health = healths(&set);
    drop(set);

    p[1].kill(false);
    let set = reopen(&p, &health, &st, |_| true);
    run_all(&set, &p, Some(&[3, 4, 6]));
    w.check(&set);
}

#[test]
fn two_members_away_from_a_raid6_set_come_back_each_by_its_map() {
    let p = probes(5);
    let st = stores(5);
    let set = fresh(Level::Raid6, &p, Some(pintent(&st)));
    let geo = set.geometry();
    let mut w = Written::default();
    set.detach(1).unwrap();
    w.put(&set, at(&geo, 0, 1, 0), 1);
    w.put(&set, at(&geo, 1, 1, 0), 2);
    set.detach(3).unwrap();
    w.put(&set, at(&geo, 3, 4, 2), 3);
    w.put(&set, at(&geo, 2, 4, 1), 4);
    set.flush().unwrap();
    assert_eq!(set.away_regions(1), Some(vec![1, 4]));
    assert_eq!(set.away_regions(3), Some(vec![4]));
    w.check(&set);
    let health = healths(&set);
    drop(set);

    // Member 3 comes back first, while member 1 is still away.
    let set = reopen(&p, &health, &st, |i| i != 1);
    run_all(&set, &p, Some(&[4]));
    w.put(&set, at(&geo, 4, 6, 0), 5);
    set.flush().unwrap();
    assert_eq!(set.away_regions(1), Some(vec![1, 4, 6]));
    w.check(&set);
    let health = healths(&set);
    drop(set);

    let set = reopen(&p, &health, &st, |_| true);
    run_all(&set, &p, Some(&[1, 4, 6]));
    assert!(healths(&set).iter().all(|h| *h == Health::Ok));
    w.check(&set);
}

#[test]
fn a_member_failing_during_its_rebuild_comes_back_by_the_same_map() {
    // The caller recorded the failure, or recorded only the mark before it.
    for (level, n, record_failure) in [
        (Level::Raid5, 4, true),
        (Level::Raid5, 4, false),
        (Level::Raid6, 5, true),
        (Level::Raid6, 5, false),
    ] {
        let p = probes(n);
        let st = stores(n);
        let set = fresh(level, &p, Some(pintent(&st)));
        let geo = set.geometry();
        let mut w = Written::default();
        set.detach(2).unwrap();
        for r in [1, 4, 6] {
            w.put(&set, at(&geo, 2, r, 0), u32::try_from(r).unwrap());
            w.put(&set, at(&geo, 0, r, 1), 10 + u32::try_from(r).unwrap());
        }
        set.flush().unwrap();
        let health = healths(&set);
        drop(set);

        let set = reopen(&p, &health, &st, |_| true);
        let mark = set.step(10).unwrap().unwrap();
        assert_eq!(mark.work, ParityWork::Rebuild(2, 16), "region 1 done");
        let at_mark = healths(&set);
        assert_eq!(at_mark[2], Health::Rebuilding(16));
        p[2].kill(true);
        let (a, b) = (at(&geo, 1, 3, 0), at(&geo, 2, 1, 2));
        set.write(a, &block(30)).unwrap();
        set.write(b, &block(31)).unwrap();
        // RAID 6 goes on without it; RAID 5 cannot lose a member being
        // rebuilt, and the flush fails.
        assert_eq!(set.flush().is_ok(), level == Level::Raid6, "{level:?}");
        assert_eq!(set.away_regions(2), Some(vec![1, 3, 4, 6]));
        let mut health = if record_failure {
            healths(&set)
        } else {
            at_mark
        };
        if record_failure {
            health[2] = Health::Failed;
        }
        drop(set);

        p[2].kill(false);
        let set = reopen(&p, &health, &st, |_| true);
        assert_eq!(set.work(), Some(ParityWork::Rebuild(2, 0)));
        run_all(&set, &p, Some(&[1, 3, 4, 6]));
        w.put(&set, a, 30);
        w.put(&set, b, 31);
        set.flush().unwrap();
        w.check(&set);
    }
}

#[test]
fn a_member_replaced_is_rebuilt_whole_and_its_map_given_up() {
    let p = probes(4);
    let st = stores(4);
    let set = fresh(Level::Raid5, &p, Some(pintent(&st)));
    let geo = set.geometry();
    let mut w = Written::default();
    set.detach(1).unwrap();
    w.put(&set, at(&geo, 0, 2, 0), 1);
    set.flush().unwrap();
    assert_eq!(kept_kind(&st[1]), Some(MapKind::Away));
    let new = Probe::new(common::parity::PAGES);
    set.replace(1, new.boxed(), [0x55; 16]).unwrap();
    assert_eq!(kept_kind(&st[1]), Some(MapKind::Off));
    assert_eq!(set.away_regions(1), None);
    let mut all = p.clone();
    all[1] = new.clone();
    assert_eq!(run_all(&set, &all, None), geo.blocks, "the whole set");
    w.check(&set);

    // The new disk fails part way through a rebuild of its own: its map
    // has every region from the mark on.
    set.detach(1).unwrap();
    let new2 = Probe::new(common::parity::PAGES);
    set.replace(1, new2.boxed(), [0x56; 16]).unwrap();
    all[1] = new2.clone();
    set.step(20).unwrap();
    assert_eq!(healths(&set)[1], Health::Rebuilding(20));
    new2.kill(true);
    let a = at(&geo, 0, 0, 1);
    set.write(a, &block(2)).unwrap();
    // RAID 5 cannot lose a member being rebuilt: the flush fails.
    assert!(set.flush().is_err());
    assert_eq!(set.away_regions(1), Some(vec![0, 2, 3, 4, 5, 6, 7]));
    let health = healths(&set);
    drop(set);
    new2.kill(false);
    let mut members = recorded(&all, &health, |_| true);
    members[1].uuid = [0x56; 16];
    let set = ParitySet::open_with(FMT, PSET, geo, members, None, Some(pintent(&st))).unwrap();
    run_all(&set, &all, Some(&[0, 2, 3, 4, 5, 6, 7]));
    w.put(&set, a, 2);
    set.flush().unwrap();
    w.check(&set);
}

#[test]
fn no_map_kept_means_a_full_rebuild() {
    let p = probes(4);
    let st = stores(4);
    let set = fresh(Level::Raid5, &p, Some(pintent(&st)));
    let geo = set.geometry();
    let mut w = Written::default();
    set.detach(1).unwrap();
    w.put(&set, at(&geo, 0, 2, 0), 1);
    set.flush().unwrap();
    let health = healths(&set);
    drop(set);
    // Another store, or none: the whole set is read.
    let other = stores(4);
    let set = reopen(&p, &health, &other, |_| true);
    assert_eq!(run_all(&set, &p, None), geo.blocks);
    w.check(&set);
    // A set opened without maps behaves as before.
    let set = fresh(Level::Raid5, &probes(4), None);
    assert_eq!(set.away_regions(0), None);
}

/// A store whose writes of bits fail while `bits` is set, and every
/// write while `all` is.
#[derive(Clone)]
struct Flaky {
    inner: MemIntentStore,
    bits: Arc<AtomicBool>,
    all: Arc<AtomicBool>,
}

impl IntentStore for Flaky {
    fn read_all(&mut self) -> io::Result<Vec<u8>> {
        self.inner.read_all()
    }
    fn write(&mut self, offset: u64, bytes: &[u8]) -> io::Result<()> {
        if self.all.load(Ordering::SeqCst) || (offset >= 64 && self.bits.load(Ordering::SeqCst)) {
            return Err(io::Error::other("injected"));
        }
        self.inner.write(offset, bytes)
    }
}

#[test]
fn a_map_that_cannot_be_kept_is_given_up_or_the_write_refused() {
    let p = probes(4);
    let st = stores(4);
    let flaky = Flaky {
        inner: st[1].clone(),
        bits: Arc::default(),
        all: Arc::default(),
    };
    let mut intent = pintent(&st);
    intent.away[1] = Some(Box::new(flaky.clone()));
    let set = fresh(Level::Raid5, &p, Some(intent));
    let geo = set.geometry();
    let mut w = Written::default();
    set.detach(1).unwrap();
    w.put(&set, at(&geo, 0, 2, 0), 1);

    // Its store cannot take a write at all: the write is refused, and no
    // member saw it.
    flaky.all.store(true, Ordering::SeqCst);
    assert!(set.write(at(&geo, 0, 5, 0), &block(2)).is_err());
    set.flush().unwrap();
    assert_eq!(value(&set, at(&geo, 0, 5, 0)), 0);
    // It can again: the map is written whole, and the write goes on.
    flaky.all.store(false, Ordering::SeqCst);
    w.put(&set, at(&geo, 0, 5, 0), 3);
    assert_eq!(set.away_regions(1), Some(vec![2, 5]));

    // Its bits cannot be kept but its header can: the map is given up
    // and the write goes on.
    flaky.bits.store(true, Ordering::SeqCst);
    w.put(&set, at(&geo, 0, 6, 0), 4);
    set.flush().unwrap();
    assert_eq!(set.away_regions(1), None);
    assert_eq!(kept_kind(&st[1]), Some(MapKind::Off));
    let health = healths(&set);
    drop(set);
    let set = reopen(&p, &health, &st, |_| true);
    assert_eq!(run_all(&set, &p, None), geo.blocks, "a full rebuild");
    w.check(&set);
}

#[test]
fn a_member_missing_at_open_gets_a_map_of_what_was_in_flight() {
    let p = probes(4);
    let st = stores(4);
    let set = fresh(Level::Raid5, &p, Some(pintent(&st)));
    let geo = set.geometry();
    let mut w = Written::default();
    w.put(&set, at(&geo, 0, 2, 0), 1);
    w.put(&set, at(&geo, 2, 2, 0), 2);
    set.flush().unwrap();
    drop(set);
    // Missing at the next open, recorded in step: the last epoch's
    // stripes are its map.
    let ok = vec![Health::Ok; 4];
    let set = reopen(&p, &ok, &st, |i| i != 1);
    assert_eq!(set.drain_events(), [SetEvent::Missing { member: 1 }]);
    assert_eq!(set.away_regions(1), Some(vec![2]));
    w.put(&set, at(&geo, 3, 7, 0), 3);
    set.flush().unwrap();
    let health = healths(&set);
    drop(set);
    // Still away at the next: the map read back goes on.
    let set = reopen(&p, &health, &st, |i| i != 1);
    assert_eq!(set.away_regions(1), Some(vec![2, 7]));
    w.put(&set, at(&geo, 0, 4, 2), 4);
    set.flush().unwrap();
    drop(set);
    let set = reopen(&p, &health, &st, |_| true);
    run_all(&set, &p, Some(&[2, 4, 7]));
    w.check(&set);
}
