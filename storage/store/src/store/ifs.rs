// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS volume at the store: created and grown with a checkpoint
//! each, and read and written by volume page through the units (the
//! format at IPL; the storage service serves it through its shared I/O
//! handle). See `docs/STORE_FORMAT.md` §12.

use crate::asp::ifs::Volume;
use crate::asp::superblock::FLAG_IFS_FORMATTED;
use crate::page::PAGE_SIZE;
use crate::store::Store;
use crate::{Refusal, Result, StoreError};

/// A growth step is this fraction of the usable pages (2%).
pub const STEP_DIVISOR: u64 = 50;
/// A new volume is this fraction of the usable pages (10%).
pub const CREATE_DIVISOR: u64 = 10;
/// The filesystem's allocation groups are this fraction of the usable
/// pages, so a volume grown to the whole ASP has about forty.
pub const AG_DIVISOR: u64 = 40;
/// Smallest allocation group: 16 MiB.
const AG_MIN: u64 = 4096;
/// Largest allocation group: 1 TiB.
const AG_MAX: u64 = 1 << 28;

fn not_in_asp() -> StoreError {
    StoreError::Refused {
        unit: None,
        reason: Refusal::IfsNotInAsp,
    }
}

impl Store {
    /// The IFS volume; empty when the IFS is not in the ASP.
    #[must_use]
    pub fn ifs_volume(&self) -> &Volume {
        self.asp.ifs_volume()
    }

    /// Whether the volume holds a formatted filesystem.
    #[must_use]
    pub fn ifs_formatted(&self) -> bool {
        self.asp.superblock().flag(FLAG_IFS_FORMATTED)
    }

    /// Set the smallest growth and the smallest volume asked for at
    /// creation, in pages (tests use small units).
    pub fn set_ifs_limits(&mut self, min: u64, create_min: u64) {
        self.ifs_limits = (min, create_min);
    }

    /// Pages one growth step adds.
    #[must_use]
    pub fn ifs_step(&self) -> u64 {
        self.asp.usable_pages() / STEP_DIVISOR
    }

    /// Pages the volume may still grow by below `threshold` percent.
    #[must_use]
    pub fn ifs_room(&self, threshold: u8) -> u64 {
        self.asp.ifs_room(threshold)
    }

    /// The allocation group size for the volume's filesystem, pages.
    #[must_use]
    pub fn ifs_ag_pages(&self) -> u64 {
        let ag = (self.asp.usable_pages() / AG_DIVISOR).clamp(AG_MIN, AG_MAX);
        ag.min((self.ifs_volume().pages() / 4).max(AG_MIN))
    }

    /// Create the volume: a tenth of the usable pages, at least 1 GiB,
    /// within `threshold`. Returns its pages.
    pub fn ifs_create(&mut self, threshold: u8) -> Result<u64> {
        self.check()?;
        if !self.ifs_volume().is_empty() {
            return Ok(self.ifs_volume().pages());
        }
        let (min, create_min) = self.ifs_limits;
        let want = (self.asp.usable_pages() / CREATE_DIVISOR).max(create_min);
        let room = self.asp.ifs_room(threshold);
        self.asp.ifs_grow(want, room, min)?;
        self.checkpoint()?;
        Ok(self.ifs_volume().pages())
    }

    /// Grow the volume by about `want` pages within `threshold`.
    /// Returns the pages added.
    pub fn ifs_grow(&mut self, want: u64, threshold: u8) -> Result<u64> {
        self.check()?;
        if self.ifs_volume().is_empty() {
            return Err(not_in_asp());
        }
        let room = self.asp.ifs_room(threshold);
        let added = self.asp.ifs_grow(want, room, self.ifs_limits.0)?;
        self.checkpoint()?;
        Ok(added)
    }

    /// Record that the volume holds a filesystem, its writes durable.
    pub fn ifs_set_formatted(&mut self) -> Result<()> {
        self.check()?;
        self.asp.io().flush()?;
        let sb = self.asp.superblock_mut();
        sb.flags |= FLAG_IFS_FORMATTED;
        self.checkpoint()
    }

    /// Read whole volume pages from `v` into `buf`.
    pub fn ifs_read(&mut self, v: u64, buf: &mut [u8]) -> Result<()> {
        let count = (buf.len() / PAGE_SIZE) as u64;
        let runs = self.ifs_volume().runs(v, count).ok_or_else(not_in_asp)?;
        let mut at = 0;
        for (pno, n) in runs {
            let got = self.asp.io().read_run(pno, n)?;
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            buf[at..at + len].copy_from_slice(&got[..len]);
            at += len;
        }
        Ok(())
    }

    /// Write whole volume pages from `v`.
    pub fn ifs_write(&mut self, v: u64, bytes: &[u8]) -> Result<()> {
        let count = (bytes.len() / PAGE_SIZE) as u64;
        let runs = self.ifs_volume().runs(v, count).ok_or_else(not_in_asp)?;
        let mut at = 0;
        for (pno, n) in runs {
            let len = usize::try_from(n).expect("small") * PAGE_SIZE;
            let buf = blockdev::AlignedBuf::from_bytes(&bytes[at..at + len]);
            self.asp.io().write_bytes(pno, &buf)?;
            at += len;
        }
        Ok(())
    }

    /// Make the volume's writes durable.
    pub fn ifs_flush(&mut self) -> Result<()> {
        self.asp.io().flush()
    }
}
