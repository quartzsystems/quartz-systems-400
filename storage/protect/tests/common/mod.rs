// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the write-intent tests share: blocks that say their value, a
//! pair of members, a set opened with a map in memory.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use blockdev::{AlignedBuf, BlockDev, MemBlockDev};
use protect::UnitState;
use protect::intent::MemIntentStore;
use protect::mirror::RESUME_CHUNK;
use protect::set::{Intent, MemJournal, MirrorMember, MirrorSet};

pub const MAGIC: [u8; 8] = *b"TESTWIBM";
pub const SET: [u8; 16] = [9; 16];

pub fn block(v: u32) -> AlignedBuf {
    let mut b = AlignedBuf::new(1);
    for (i, x) in b.iter_mut().enumerate() {
        *x = u8::try_from((v as usize * 7 + i) % 251).unwrap();
    }
    b[..4].copy_from_slice(&v.to_le_bytes());
    b
}

pub fn value(dev: &dyn BlockDev, lba: u64) -> u32 {
    let mut b = AlignedBuf::new(1);
    dev.read(lba, &mut b).unwrap();
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

pub fn member(dev: Box<dyn BlockDev>, id: u8, state: UnitState) -> MirrorMember {
    MirrorMember {
        uuid: [id; 16],
        state,
        dev: Some(dev),
    }
}

pub fn intent(store: &MemIntentStore, fresh: bool, region: u64, delay: Duration) -> Intent {
    let mut i = Intent::new(MAGIC, Box::new(store.clone()));
    i.region_blocks = region;
    i.clear_delay = delay;
    i.fresh = fresh;
    i
}

pub fn pair(blocks: u64) -> (Arc<MemBlockDev>, Arc<MemBlockDev>) {
    (
        Arc::new(MemBlockDev::new(blocks)),
        Arc::new(MemBlockDev::new(blocks)),
    )
}

pub fn open(a: &Arc<MemBlockDev>, b: &Arc<MemBlockDev>, blocks: u64, intent: Intent) -> MirrorSet {
    MirrorSet::open_with(
        SET,
        blocks,
        vec![
            member(Box::new(Arc::clone(a)), 1, UnitState::Active),
            member(Box::new(Arc::clone(b)), 2, UnitState::Active),
        ],
        Some(intent),
    )
    .unwrap()
}

pub fn resync_all(set: &MirrorSet) -> u64 {
    loop {
        let p = set.resync_step(4).unwrap().expect("a copy");
        if p.complete {
            return p.chunks_copied;
        }
    }
}

pub const REGION: u64 = 2 * RESUME_CHUNK;

pub fn resume_all(set: &MirrorSet, member: usize, journal: &mut MemJournal) -> u64 {
    let mut r = set.resume(member, journal).unwrap();
    while !set.resume_step(&mut r, journal, 3).unwrap().complete {}
    r.copied()
}
