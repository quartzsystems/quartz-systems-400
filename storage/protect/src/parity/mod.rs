// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Device parity protection in software: RAID 5 and RAID 6 sets of
//! units. Each member keeps its own rows; a [`ParityDev`] stands in
//! place of the member's device, and every page passes through the
//! set's [`Engine`]. The layout is `docs/STORE_FORMAT.md` §11; what
//! names its owner on disk comes from the caller's [`Format`].

pub mod engine;
pub mod geometry;
pub mod gf;
mod level;
pub mod ppl;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use blockdev::{BLOCK_SIZE, BlockDev, check_range};

use crate::PAGE_SIZE;
pub use engine::{Engine, EngineEvent, Fault, Health, Member};
pub use geometry::Geometry;
pub use level::{Level, MAX_MEMBERS};

/// What a caller's format fixes for its sets.
#[derive(Debug, Clone, Copy)]
pub struct Format {
    /// Magic of a partial-parity slot header.
    pub ppl_magic: [u8; 8],
    /// Magic of the state page.
    pub state_magic: [u8; 8],
    /// Whether a page read from a member is damaged, for a caller whose
    /// pages can tell (a checksum). The engine then computes the page
    /// from its stripe and rewrites it. A caller whose pages carry
    /// nothing to check answers `false`.
    pub suspicious: fn(&[u8]) -> bool,
}

/// A member's device as the layer above sees it.
pub struct ParityDev {
    engine: Arc<Mutex<Engine>>,
    member: usize,
    blocks: u64,
}

fn lock(e: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(PoisonError::into_inner)
}

impl BlockDev for ParityDev {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let mut e = lock(&self.engine);
        for (i, chunk) in (0..count).zip(buf.chunks_mut(PAGE_SIZE)) {
            chunk.copy_from_slice(&e.read_page(self.member, lba + i)?);
        }
        Ok(())
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let mut e = lock(&self.engine);
        for (i, chunk) in (0..count).zip(buf.chunks(PAGE_SIZE)) {
            e.write_page(self.member, lba + i, chunk)?;
        }
        Ok(())
    }

    fn flush(&self) -> blockdev::Result<()> {
        lock(&self.engine).flush()
    }
}

/// One set at run time.
pub struct SetHandle {
    /// Set number.
    pub set: u8,
    /// The engine its members' devices share.
    pub engine: Arc<Mutex<Engine>>,
}

impl SetHandle {
    /// The engine, locked.
    pub fn lock(&self) -> MutexGuard<'_, Engine> {
        lock(&self.engine)
    }

    /// A member's device for the layer above.
    #[must_use]
    pub fn device(&self, member: usize, blocks: u64) -> Box<dyn BlockDev> {
        Box::new(ParityDev {
            engine: Arc::clone(&self.engine),
            member,
            blocks,
        })
    }
}
