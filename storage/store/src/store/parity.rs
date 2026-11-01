// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Device parity protection at the store: each change to the sets is
//! published by a checkpoint before anything is written under it.

use crate::asp::parity::Fault;
use crate::asp::parity::ops::{ParityProgress, SetInfo};
use crate::asp::parity::table::Level;
use crate::store::Store;
use crate::{Refusal, Result, StoreError};

/// The mark is recorded every `1/MARK_EVERY` of a set's blocks ...
pub const MARK_EVERY: u64 = 1000;
/// ... and never more often than every `MARK_MIN` blocks.
pub const MARK_MIN: u64 = 32;

impl Store {
    /// Refuse a unit that is in a parity set.
    pub(super) fn not_parity(&self, unit: u16) -> Result<()> {
        match self.asp.parity_set_of(unit) {
            Some(set) => Err(StoreError::refused(unit, Refusal::InParitySet(set))),
            None => Ok(()),
        }
    }

    /// Start device parity protection over units; returns the set. It
    /// synchronizes through [`parity_step`](Self::parity_step).
    pub fn start_parity(&mut self, level: Level, units: &[u16]) -> Result<u8> {
        self.check()?;
        let set = self.asp.start_parity(level, units)?;
        self.checkpoint()?;
        Ok(set)
    }

    /// Stop device parity protection for a set.
    pub fn stop_parity(&mut self, set: u8) -> Result<()> {
        self.check()?;
        self.asp.stop_parity(set)?;
        self.checkpoint()
    }

    /// Include a unit in a set.
    pub fn include_parity(&mut self, set: u8, unit: u16) -> Result<()> {
        self.check()?;
        self.asp.include_parity(set, unit)?;
        self.checkpoint()
    }

    /// Exclude a unit from its set.
    pub fn exclude_parity(&mut self, unit: u16) -> Result<()> {
        self.check()?;
        self.asp.exclude_parity(unit)?;
        self.checkpoint()
    }

    /// Run up to `budget` blocks of the parity work due. A checkpoint
    /// publishes the end of each piece, and the mark each time it has
    /// moved a [`MARK_EVERY`]th of the set (at least [`MARK_MIN`] blocks)
    /// past the last one recorded, so a crash redoes little.
    pub fn parity_step(&mut self, budget: u64) -> Result<Option<ParityProgress>> {
        self.check()?;
        let Some(p) = self.asp.parity_step(budget)? else {
            return Ok(None);
        };
        let recorded = self
            .asp
            .superblock()
            .parity
            .work
            .filter(|w| w.set == p.set && w.kind == p.kind)
            .map_or(0, |w| w.done);
        if p.complete || p.done.saturating_sub(recorded) >= (p.blocks / MARK_EVERY).max(MARK_MIN) {
            self.checkpoint()?;
        }
        Ok(Some(p))
    }

    /// Every set, for the displays.
    #[must_use]
    pub fn parity_sets(&self) -> Vec<SetInfo> {
        self.asp.parity_sets()
    }

    /// The set a unit is in.
    #[must_use]
    pub fn parity_set_of(&self, unit: u16) -> Option<u8> {
        self.asp.parity_set_of(unit)
    }

    /// Stripes of a set whose parity does not match.
    pub fn parity_verify(&mut self, set: u8) -> Result<Vec<u64>> {
        self.asp.parity_verify(set)
    }

    /// The test hook: make the next epoch of a set stop after its data.
    pub fn parity_fault(&mut self, set: u8, fault: Option<Fault>) {
        self.asp.parity_fault(set, fault);
    }
}
