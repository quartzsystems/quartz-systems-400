// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Protected sets as block devices, with no store: a mirrored set and a
//! parity set written, a member lost, read back, the member replaced
//! (by hand and from a hot spare) and brought back in step.

use std::sync::Arc;

use blockdev::{AlignedBuf, BlockDev, MemBlockDev};
use protect::UnitState;
use protect::mirror::{RESUME_CHUNK, ResumeState};
use protect::parity::{Format, Health, Level};
use protect::set::{
    MemJournal, MirrorMember, MirrorSet, ParityMember, ParitySet, ParityWork, ResumeJournal,
    SetEvent, Window,
};
use protect::spare::Spare;

const PAGE: usize = 4096;

/// A caller's own constants: nothing here is the object store's.
const FMT: Format = Format {
    ppl_magic: *b"DEMO-PPL",
    state_magic: *b"DEMO-PST",
    suspicious: |_| false,
};

/// A member's identity from its place.
fn id(i: usize) -> [u8; 16] {
    [u8::try_from(i).unwrap() + 1; 16]
}

fn block(n: u64) -> AlignedBuf {
    let mut b = AlignedBuf::new(1);
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::try_from((n * 31 + u64::try_from(i).unwrap()) % 251).unwrap();
    }
    b
}

fn fill(dev: &dyn BlockDev, blocks: impl Iterator<Item = u64>) {
    for n in blocks {
        dev.write(n, &block(n)).unwrap();
    }
    dev.flush().unwrap();
}

fn check(dev: &dyn BlockDev, blocks: impl Iterator<Item = u64>) {
    let mut got = AlignedBuf::new(1);
    for n in blocks {
        dev.read(n, &mut got).unwrap();
        assert_eq!(&got[..], &block(n)[..], "block {n}");
    }
}

fn member(dev: &Arc<MemBlockDev>, id: u8, state: UnitState) -> MirrorMember {
    MirrorMember {
        uuid: [id; 16],
        state,
        dev: Some(Box::new(Arc::clone(dev))),
    }
}

fn resume_all(set: &MirrorSet, member: usize, journal: &mut MemJournal) {
    let mut r = set.resume(member, journal).unwrap();
    while !set.resume_step(&mut r, journal, 2).unwrap().complete {}
}

#[test]
#[allow(clippy::many_single_char_names)] // two disks, a third, a resume
fn a_mirrored_set_loses_a_member_and_gets_it_back() {
    let blocks = 3 * RESUME_CHUNK + 40;
    let a = Arc::new(MemBlockDev::new(blocks));
    let b = Arc::new(MemBlockDev::new(blocks + 8));
    let set = MirrorSet::open(
        [9; 16],
        MirrorSet::blocks_for(&[a.as_ref(), b.as_ref()]),
        vec![
            member(&a, 1, UnitState::Active),
            member(&b, 2, UnitState::Active),
        ],
    )
    .unwrap();
    assert_eq!(set.blocks(), blocks);
    fill(&set, 0..200);
    assert_eq!(
        a.flushed_image(),
        b.flushed_image()[..a.flushed_image().len()]
    );

    // A member is pulled: the set serves what it held and takes more.
    let pulled = set.detach(1).unwrap();
    assert!(pulled.is_some());
    assert_eq!(set.members()[1].state, UnitState::Missing);
    check(&set, 0..200);
    fill(&set, 200..400);
    assert!(set.detach(0).is_err(), "the last active member stays");

    // A new disk in its place, brought in step while writes go on.
    let c = Arc::new(MemBlockDev::new(blocks));
    set.replace(1, Box::new(Arc::clone(&c)), [3; 16]).unwrap();
    assert_eq!(set.members()[1].state, UnitState::Resuming);
    let mut journal = MemJournal::default();
    let mut r = set.resume(1, &mut journal).unwrap();
    let p = set.resume_step(&mut r, &mut journal, 1).unwrap();
    assert!(!p.complete && p.chunks_done == 1);
    fill(&set, 400..420);
    assert!(journal.saved.is_some());
    while !set.resume_step(&mut r, &mut journal, 8).unwrap().complete {}
    assert!(journal.saved.is_none());
    let m = set.members();
    assert_eq!((m[1].state, m[1].uuid), (UnitState::Active, [3; 16]));
    assert_eq!(a.flushed_image(), c.flushed_image());
    check(&set, 0..420);

    // The first member alone can go now.
    set.detach(0).unwrap();
    check(&set, 0..420);
}

#[test]
fn a_failing_member_is_taken_out_and_a_hot_spare_takes_its_place() {
    let blocks = RESUME_CHUNK + 10;
    let a = Arc::new(MemBlockDev::new(blocks));
    let b = Arc::new(MemBlockDev::new(blocks));
    let set = MirrorSet::open(
        [9; 16],
        blocks,
        vec![
            member(&a, 1, UnitState::Active),
            member(&b, 2, UnitState::Active),
        ],
    )
    .unwrap();
    fill(&set, 0..50);
    b.fail_after(Some(0));
    fill(&set, 50..60);
    assert_eq!(set.members()[1].state, UnitState::NotOperational);
    let events = set.drain_events();
    assert!(matches!(&events[..], [SetEvent::Failed { member: 1, .. }]));
    assert_eq!(set.failed(), [1]);

    let mut spares: Vec<Spare> = [(7u8, blocks * 2), (8, blocks)]
        .into_iter()
        .map(|(id, n)| Spare {
            uuid: [id; 16],
            dev: Box::new(MemBlockDev::new(n)),
        })
        .collect();
    assert_eq!(set.take_spare(&mut spares), Some((1, [8; 16])));
    assert_eq!(spares.len(), 1);
    let mut journal = MemJournal::default();
    resume_all(&set, 1, &mut journal);
    assert!(set.members().iter().all(|m| m.state == UnitState::Active));
    set.detach(0).unwrap();
    check(&set, 0..60);
    // The other spare takes the detached member's place in turn.
    assert_eq!(set.take_spare(&mut spares), Some((0, [7; 16])));
    resume_all(&set, 0, &mut journal);
    check(&set, 0..60);
    assert!(set.take_spare(&mut spares).is_none());
}

#[test]
fn a_resume_goes_on_where_the_journal_says() {
    let blocks = 4 * RESUME_CHUNK;
    let a = Arc::new(MemBlockDev::new(blocks));
    let b = Arc::new(MemBlockDev::new(blocks));
    fill(a.as_ref(), (0..blocks).step_by(7));
    let open = || {
        MirrorSet::open(
            [9; 16],
            blocks,
            vec![
                member(&a, 1, UnitState::Active),
                member(&b, 2, UnitState::Resuming),
            ],
        )
        .unwrap()
    };
    let mut journal = MemJournal::default();
    let set = open();
    let mut r = set.resume(1, &mut journal).unwrap();
    set.resume_step(&mut r, &mut journal, 3).unwrap();
    let saved: ResumeState = journal.saved.clone().unwrap();
    assert_eq!(saved.chunks_done(), 3);
    // The process ends here; the next one opens the set as recorded.
    drop(set);
    let set = open();
    let mut r = set.resume(1, &mut journal).unwrap();
    assert_eq!(r.progress().chunks_done, 3);
    let reads = a.stats().reads;
    assert!(set.resume_step(&mut r, &mut journal, 8).unwrap().complete);
    assert_eq!(a.stats().reads - reads, 1, "one chunk was left");
    assert_eq!(a.flushed_image(), b.flushed_image());
    assert!(journal.load([2; 16]).is_none());
}

#[test]
fn open_reads_the_record_against_what_was_found() {
    let a = Arc::new(MemBlockDev::new(64));
    let b = Arc::new(MemBlockDev::new(64));
    // One device is not there.
    let set = MirrorSet::open(
        [1; 16],
        64,
        vec![
            member(&a, 1, UnitState::Active),
            MirrorMember {
                uuid: [2; 16],
                state: UnitState::Active,
                dev: None,
            },
        ],
    )
    .unwrap();
    assert_eq!(set.drain_events(), [SetEvent::Missing { member: 1 }]);
    // It is back, recorded missing: stale until resumed.
    let set = MirrorSet::open(
        [1; 16],
        64,
        vec![
            member(&a, 1, UnitState::Active),
            member(&b, 2, UnitState::Missing),
        ],
    )
    .unwrap();
    assert_eq!(set.drain_events(), [SetEvent::Stale { member: 1 }]);
    assert_eq!(set.members()[1].state, UnitState::Suspended);
    // Only the stale half: no set.
    assert!(
        MirrorSet::open(
            [1; 16],
            64,
            vec![
                MirrorMember {
                    uuid: [1; 16],
                    state: UnitState::Active,
                    dev: None,
                },
                member(&b, 2, UnitState::Suspended),
            ],
        )
        .is_err()
    );
    // Too small for the set.
    assert!(
        MirrorSet::open(
            [1; 16],
            128,
            vec![
                member(&a, 1, UnitState::Active),
                member(&b, 2, UnitState::Active),
            ],
        )
        .is_err()
    );
}

#[test]
fn a_set_over_windows_leaves_each_disk_its_label() {
    let a = Arc::new(MemBlockDev::new(72));
    let b = Arc::new(MemBlockDev::new(72));
    a.write(0, &block(1000)).unwrap();
    let win = |d: &Arc<MemBlockDev>| -> Box<dyn BlockDev> {
        Box::new(Window::new(Arc::clone(d), 8, 64).unwrap())
    };
    let set = MirrorSet::open(
        [1; 16],
        64,
        vec![
            MirrorMember {
                uuid: [1; 16],
                state: UnitState::Active,
                dev: Some(win(&a)),
            },
            MirrorMember {
                uuid: [2; 16],
                state: UnitState::Active,
                dev: Some(win(&b)),
            },
        ],
    )
    .unwrap();
    fill(&set, 0..64);
    let mut got = AlignedBuf::new(1);
    a.read(0, &mut got).unwrap();
    assert_eq!(&got[..], &block(1000)[..], "the label");
    b.read(8, &mut got).unwrap();
    assert_eq!(&got[..], &block(0)[..]);
}

fn parity_devs(n: usize, blocks: u64) -> Vec<Arc<MemBlockDev>> {
    (0..n).map(|_| Arc::new(MemBlockDev::new(blocks))).collect()
}

fn sync_all(set: &ParitySet) {
    while set.step(64).unwrap().is_some() {}
}

#[test]
fn a_parity_set_loses_a_member_and_rebuilds_it() {
    for (level, n) in [(Level::Raid5, 3), (Level::Raid6, 5)] {
        let devs = parity_devs(n, 900);
        let set = ParitySet::create(
            FMT,
            [4; 16],
            level,
            0,
            devs.iter()
                .enumerate()
                .map(|(i, d)| (id(i), Box::new(Arc::clone(d)) as Box<dyn BlockDev>))
                .collect(),
        )
        .unwrap();
        assert_eq!(set.work(), Some(ParityWork::Sync(0)));
        let total = set.blocks();
        assert!(total > 300, "{total}");
        fill(&set, (0..total).step_by(3));
        sync_all(&set);
        assert_eq!(set.sync(), None);
        assert!(set.verify().unwrap().is_empty());

        // A member is pulled: every block still reads, writes go on.
        set.detach(1).unwrap();
        assert_eq!(set.failed(), [1]);
        check(&set, (0..total).step_by(3));
        fill(&set, (1..total).step_by(3));
        assert!(
            set.drain_events()
                .iter()
                .any(|e| matches!(e, SetEvent::Failed { member: 1, .. }))
        );

        // A hot spare takes its place and is rebuilt.
        let mut spares = vec![Spare {
            uuid: [0x55; 16],
            dev: Box::new(MemBlockDev::new(900)),
        }];
        assert_eq!(set.take_spare(&mut spares), Some((1, [0x55; 16])));
        assert_eq!(set.work(), Some(ParityWork::Rebuild(1, 0)));
        sync_all(&set);
        assert!(set.members().iter().all(|m| m.health == Health::Ok));
        assert!(set.verify().unwrap().is_empty());

        // Now another can go and nothing is lost.
        set.detach(0).unwrap();
        check(&set, (0..total).step_by(3));
        check(&set, (1..total).step_by(3));
    }
}

#[test]
fn a_parity_set_opens_again_as_recorded() {
    let devs = parity_devs(4, 900);
    let boxed = |d: &Arc<MemBlockDev>| -> Box<dyn BlockDev> { Box::new(Arc::clone(d)) };
    let set = ParitySet::create(
        FMT,
        [4; 16],
        Level::Raid5,
        2,
        devs.iter()
            .enumerate()
            .map(|(i, d)| (id(i), boxed(d)))
            .collect(),
    )
    .unwrap();
    let geo = set.geometry();
    let total = set.blocks();
    fill(&set, 0..total / 2);
    sync_all(&set);
    // The head of every member is the caller's.
    let mut got = AlignedBuf::new(2);
    devs[0].read(0, &mut got).unwrap();
    assert!(got.iter().all(|&b| b == 0));
    drop(set);

    // Reopened as recorded, one device gone: exposed, and served.
    let members = |missing: Option<usize>, health: Health| {
        devs.iter()
            .enumerate()
            .map(|(i, d)| ParityMember {
                uuid: id(i),
                health: if missing == Some(i) {
                    health
                } else {
                    Health::Ok
                },
                dev: (missing != Some(i)).then(|| boxed(d)),
            })
            .collect::<Vec<_>>()
    };
    let set = ParitySet::open(FMT, [4; 16], geo, members(Some(2), Health::Ok), None).unwrap();
    assert_eq!(set.drain_events(), [SetEvent::Missing { member: 2 }]);
    check(&set, 0..total / 2);
    fill(&set, total / 2..total / 2 + 30);
    drop(set);

    // The caller never recorded the failure; the others did. The disk
    // is back: it is stale and rebuilt.
    let all: Vec<ParityMember> = devs
        .iter()
        .enumerate()
        .map(|(i, d)| ParityMember {
            uuid: id(i),
            health: Health::Ok,
            dev: Some(boxed(d)),
        })
        .collect();
    let set = ParitySet::open(FMT, [4; 16], geo, all, None).unwrap();
    assert_eq!(set.drain_events(), [SetEvent::Stale { member: 2 }]);
    assert_eq!(set.work(), Some(ParityWork::Rebuild(2, 0)));
    sync_all(&set);
    assert!(set.verify().unwrap().is_empty());
    check(&set, 0..total / 2 + 30);

    // Another caller's constants do not read these slots as its own, and
    // two members gone is more than RAID 5 can lose.
    let two_gone: Vec<ParityMember> = devs
        .iter()
        .enumerate()
        .map(|(i, d)| ParityMember {
            uuid: id(i),
            health: Health::Ok,
            dev: (i > 1).then(|| boxed(d)),
        })
        .collect();
    assert!(ParitySet::open(FMT, [4; 16], geo, two_gone, None).is_err());
    assert_eq!(PAGE, protect::PAGE_SIZE);
}
