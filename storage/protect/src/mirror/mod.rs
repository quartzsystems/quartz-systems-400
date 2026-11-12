// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! I/O over the halves of a mirrored set: written to every half that
//! takes writes, read from one, a failing half noted for the caller to
//! take out of service, a block that fails the caller's check on one
//! half read from the other and rewritten.
//!
//! The caller holds the members (behind whatever lock it has) and lends
//! them for one transfer as [`Half`] values; nothing here keeps state.
//! A transfer that finds a half failing records it in a [`Fails`] list
//! instead of changing its state, so the caller can do that once its
//! own read lock is let go.

mod resume;

use std::sync::atomic::{AtomicU64, Ordering};

use blockdev::{AlignedBuf, BlockDev, BlockError};

use crate::UnitState;
pub use resume::{RESUME_CHUNK, ResumeIo, ResumeProgress, ResumeState, SAVE_EVERY, copy_step};

/// One half as a transfer sees it.
#[derive(Clone, Copy)]
pub struct Half<'a> {
    /// The caller's number for it.
    pub unit: u16,
    /// Its state.
    pub state: UnitState,
    /// Its device, when attached.
    pub dev: Option<&'a dyn BlockDev>,
}

/// A half to take out of service: the set, the unit, what failed.
pub type Fails = Vec<(u16, u16, String)>;

/// Why a transfer on a set failed.
#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    /// The device error of the half that could not be done without.
    #[error(transparent)]
    Block(#[from] BlockError),
    /// No half of the set can serve the transfer.
    #[error("no half of the set is available")]
    NoHalf,
}

/// Why a checked read failed.
#[derive(Debug)]
pub enum CheckedError<E> {
    /// The device error of the half that could not be done without.
    Block(BlockError),
    /// The caller's check failed on the block as every half has it.
    Check(E),
    /// No half of the set can serve the read.
    NoHalf,
}

/// The buffer a checked read fills: bytes, or a caller's block type that
/// remembers what its check found.
pub trait BlockBuf {
    /// The whole block, to read into.
    fn bytes_mut(&mut self) -> &mut [u8];
    /// The whole block.
    fn bytes(&self) -> &[u8];
}

impl BlockBuf for [u8] {
    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl BlockBuf for AlignedBuf {
    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
    fn bytes(&self) -> &[u8] {
        self
    }
}

/// The halves of one set, lent for a transfer. `set` is the caller's
/// number for the set, carried into [`Fails`].
#[derive(Clone, Copy)]
pub struct Halves<'a> {
    /// The caller's number for the set.
    pub set: u16,
    /// Its members, the primary first.
    pub halves: &'a [Half<'a>],
}

impl<'a> Halves<'a> {
    /// Members that may serve reads, the preferred one first. `turn`
    /// alternates between two readers.
    #[must_use]
    pub fn readers(&self, turn: &AtomicU64) -> Vec<Half<'a>> {
        let mut r: Vec<Half<'a>> = self
            .halves
            .iter()
            .filter(|h| h.state.readable() && h.dev.is_some())
            .copied()
            .collect();
        if r.len() > 1 && turn.fetch_add(1, Ordering::Relaxed).is_multiple_of(2) {
            r.rotate_left(1);
        }
        r
    }

    /// Members that must be written.
    #[must_use]
    pub fn writers(&self) -> Vec<Half<'a>> {
        self.halves
            .iter()
            .filter(|h| h.state.writable() && h.dev.is_some())
            .copied()
            .collect()
    }

    /// Whether the set has a writable half besides `unit`.
    #[must_use]
    pub fn others(&self, unit: u16) -> bool {
        self.halves
            .iter()
            .any(|h| h.unit != unit && h.state.writable() && h.dev.is_some())
    }

    /// Record a failing half to take out of service; whether the set
    /// can go on without it (no other half failed in this call).
    pub fn fail(&self, unit: u16, what: String, fails: &mut Fails) -> bool {
        let others = self.halves.iter().any(|h| {
            h.unit != unit
                && h.state.writable()
                && h.dev.is_some()
                && !fails.iter().any(|f| f.1 == h.unit)
        });
        if others {
            fails.push((self.set, unit, what));
        }
        others
    }

    /// Read `count` blocks at `lba` as they are, failing over between
    /// halves. `at` is the block's number as the caller's messages
    /// name it.
    pub fn read_run(
        &self,
        lba: u64,
        count: u64,
        at: u64,
        turn: &AtomicU64,
        fails: &mut Fails,
    ) -> Result<AlignedBuf, MirrorError> {
        let mut first_err = None;
        for h in self.readers(turn) {
            let mut buf = AlignedBuf::new(usize::try_from(count).expect("count fits"));
            match h.dev.expect("reader is attached").read(lba, &mut buf) {
                Ok(()) => return Ok(buf),
                Err(e) => {
                    let what = format!("read {count} pages at {at}: {e}");
                    if !self.fail(h.unit, what, fails) {
                        return Err(e.into());
                    }
                    first_err.get_or_insert(e);
                }
            }
        }
        Err(first_err.map_or(MirrorError::NoHalf, MirrorError::from))
    }

    /// Read into `buf` and run the caller's `check` on what came. A
    /// block that fails on one half is read from the next; when a later
    /// half serves it the first is rewritten, and its unit is the
    /// answer.
    pub fn read_checked<B: BlockBuf + ?Sized, E>(
        &self,
        lba: u64,
        buf: &mut B,
        at: u64,
        turn: &AtomicU64,
        fails: &mut Fails,
        mut check: impl FnMut(&B) -> Result<(), E>,
    ) -> Result<Option<u16>, CheckedError<E>> {
        let readers = self.readers(turn);
        let mut first_err = None;
        for (i, h) in readers.iter().enumerate() {
            match h
                .dev
                .expect("reader is attached")
                .read(lba, buf.bytes_mut())
            {
                Ok(()) => match check(buf) {
                    Ok(()) => {
                        let healed = if i > 0 {
                            self.heal(readers[0], lba, at, buf.bytes(), fails)
                        } else {
                            None
                        };
                        return Ok(healed);
                    }
                    Err(e) => {
                        first_err.get_or_insert(CheckedError::Check(e));
                    }
                },
                Err(e) => {
                    let what = format!("read page {at}: {e}");
                    if !self.fail(h.unit, what, fails) {
                        return Err(CheckedError::Block(e));
                    }
                    first_err.get_or_insert(CheckedError::Block(e));
                }
            }
        }
        Err(first_err.unwrap_or(CheckedError::NoHalf))
    }

    /// Rewrite a block on the half that could not serve it.
    fn heal(&self, h: Half<'a>, lba: u64, at: u64, block: &[u8], fails: &mut Fails) -> Option<u16> {
        if !h.state.writable() || fails.iter().any(|f| f.1 == h.unit) {
            return None;
        }
        let dev = h.dev?;
        match dev.write(lba, block) {
            Ok(()) => Some(h.unit),
            Err(e) => {
                self.fail(h.unit, format!("rewrite page {at}: {e}"), fails);
                None
            }
        }
    }

    /// Write whole blocks at `lba` to every half that takes writes.
    pub fn write(
        &self,
        lba: u64,
        bytes: &[u8],
        at: u64,
        fails: &mut Fails,
    ) -> Result<(), MirrorError> {
        let writers = self.writers();
        if writers.is_empty() {
            return Err(MirrorError::NoHalf);
        }
        let mut failed = Vec::new();
        for h in &writers {
            if let Err(e) = h.dev.expect("writer is attached").write(lba, bytes) {
                failed.push((h.unit, e));
            }
        }
        if failed.len() == writers.len() {
            return Err(failed.remove(0).1.into());
        }
        for (unit, e) in failed {
            self.fail(unit, format!("write page {at}: {e}"), fails);
        }
        Ok(())
    }

    /// Flush every half that takes writes, waiting for all of them.
    pub fn flush(&self, fails: &mut Fails) -> Result<(), MirrorError> {
        let writers = self.writers();
        if writers.is_empty() {
            return Err(MirrorError::NoHalf);
        }
        let mut failed = Vec::new();
        for h in &writers {
            if let Err(e) = h.dev.expect("writer is attached").flush() {
                failed.push((h.unit, e));
            }
        }
        if failed.len() == writers.len() {
            return Err(failed.remove(0).1.into());
        }
        for (unit, e) in failed {
            self.fail(unit, format!("flush: {e}"), fails);
        }
        Ok(())
    }

    /// Tell every half that takes writes a run is unused.
    pub fn discard(&self, lba: u64, len: u64) -> Result<(), BlockError> {
        for h in self.writers() {
            h.dev.expect("writer is attached").discard(lba, len)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use blockdev::MemBlockDev;

    use super::*;
    use crate::PAGE_SIZE;

    fn page(fill: u8) -> AlignedBuf {
        let mut b = AlignedBuf::new(1);
        b.fill(fill);
        b
    }

    #[test]
    fn writes_reach_both_and_reads_alternate() {
        let (a, b) = (
            Arc::new(MemBlockDev::new(16)),
            Arc::new(MemBlockDev::new(16)),
        );
        let halves = [
            Half {
                unit: 0,
                state: UnitState::Active,
                dev: Some(a.as_ref()),
            },
            Half {
                unit: 1,
                state: UnitState::Active,
                dev: Some(b.as_ref()),
            },
        ];
        let set = Halves {
            set: 7,
            halves: &halves,
        };
        let mut fails = Fails::new();
        set.write(3, &page(0xAB), 3, &mut fails).unwrap();
        let mut got = page(0);
        a.read(3, &mut got).unwrap();
        assert_eq!(got[0], 0xAB);
        b.read(3, &mut got).unwrap();
        assert_eq!(got[PAGE_SIZE - 1], 0xAB);
        let turn = AtomicU64::new(0);
        let first = set.readers(&turn)[0].unit;
        let second = set.readers(&turn)[0].unit;
        assert_ne!(first, second);
        assert!(fails.is_empty());
    }

    #[test]
    fn a_suspended_half_is_neither_read_nor_written() {
        let (a, b) = (
            Arc::new(MemBlockDev::new(16)),
            Arc::new(MemBlockDev::new(16)),
        );
        let halves = [
            Half {
                unit: 0,
                state: UnitState::Active,
                dev: Some(a.as_ref()),
            },
            Half {
                unit: 1,
                state: UnitState::Suspended,
                dev: Some(b.as_ref()),
            },
        ];
        let set = Halves {
            set: 0,
            halves: &halves,
        };
        let mut fails = Fails::new();
        set.write(0, &page(1), 0, &mut fails).unwrap();
        let mut got = page(9);
        b.read(0, &mut got).unwrap();
        assert_eq!(got[0], 0, "the suspended half kept its block");
        let turn = AtomicU64::new(0);
        for _ in 0..4 {
            assert_eq!(set.read_run(0, 1, 0, &turn, &mut fails).unwrap()[0], 1);
        }
        assert!(!set.others(0) && set.others(1));
    }

    #[test]
    fn a_block_that_fails_the_check_is_rewritten_from_the_other_half() {
        let (a, b) = (
            Arc::new(MemBlockDev::new(16)),
            Arc::new(MemBlockDev::new(16)),
        );
        a.write(5, &page(0xEE)).unwrap();
        b.write(5, &page(0x11)).unwrap();
        let halves = [
            Half {
                unit: 0,
                state: UnitState::Active,
                dev: Some(a.as_ref()),
            },
            Half {
                unit: 1,
                state: UnitState::Active,
                dev: Some(b.as_ref()),
            },
        ];
        let set = Halves {
            set: 0,
            halves: &halves,
        };
        let mut fails = Fails::new();
        // The second read starts at unit 0 (turn 1 does not rotate).
        let turn = AtomicU64::new(1);
        let mut buf = page(0);
        let healed = set
            .read_checked(5, &mut buf, 5, &turn, &mut fails, |b: &AlignedBuf| {
                if b[0] == 0x11 { Ok(()) } else { Err("bad") }
            })
            .unwrap();
        assert_eq!(healed, Some(0));
        let mut got = page(0);
        a.read(5, &mut got).unwrap();
        assert_eq!(got[0], 0x11);
        // Bad on both: the first half's verdict comes back.
        let r = set.read_checked(5, &mut buf, 5, &turn, &mut fails, |_: &AlignedBuf| {
            Err("worse")
        });
        assert!(matches!(r, Err(CheckedError::Check("worse"))));
        // No reader at all.
        let none = Halves {
            set: 0,
            halves: &[],
        };
        assert!(matches!(
            none.read_run(0, 1, 0, &turn, &mut fails),
            Err(MirrorError::NoHalf)
        ));
    }
}
