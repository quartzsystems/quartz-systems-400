// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The change log tree: records appended under rising change numbers,
//! read from a number on, trimmed to a count.

use store::txn::Txn;

use super::db::{Chunks, DirDb};
use super::error::Result;
use super::format::{ChunkKey, SLOT_LOG};
use super::record::ChangeRec;

impl DirDb {
    /// Append `rec`; returns its change number. With a limit set, the
    /// oldest records past it go.
    ///
    /// # Errors
    ///
    /// A record past the limit, the object store's.
    pub fn log_append(&self, txn: &mut Txn<'_>, rec: &ChangeRec) -> Result<u64> {
        let bytes = rec.encode()?;
        let mut a = self.attr(txn)?;
        let n = a.next_change;
        a.next_change += 1;
        a.log_count += 1;
        if a.log_first == 0 {
            a.log_first = n;
        }
        let max = a.log_max;
        self.save_attr(txn, &a)?;
        self.write_record(txn, SLOT_LOG, n, &bytes)?;
        if max > 0 {
            self.log_trim(txn, max)?;
        }
        Ok(n)
    }

    /// Up to `max` records from change number `from` on, ascending.
    ///
    /// # Errors
    ///
    /// The object store's, or a damaged record.
    pub fn log_read(
        &self,
        txn: &mut Txn<'_>,
        from: u64,
        max: usize,
    ) -> Result<Vec<(u64, ChangeRec)>> {
        let t: Chunks = self.tree(txn, SLOT_LOG)?;
        let mut numbers = Vec::new();
        for r in t.range(txn, ChunkKey::first(from)..)? {
            let (k, _) = r?;
            if k.chunk == 0 {
                if numbers.len() == max {
                    break;
                }
                numbers.push(k.id);
            }
        }
        let mut out = Vec::with_capacity(numbers.len());
        for n in numbers {
            if let Some(b) = self.read_record(txn, SLOT_LOG, n)? {
                out.push((n, ChangeRec::decode(&b)?));
            }
        }
        Ok(out)
    }

    /// Drop the oldest records until at most `keep` are left.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn log_trim(&self, txn: &mut Txn<'_>, keep: u64) -> Result<()> {
        let mut a = self.attr(txn)?;
        while a.log_count > keep {
            self.delete_record(txn, SLOT_LOG, a.log_first)?;
            a.log_count -= 1;
            a.log_first = if a.log_count == 0 {
                0
            } else {
                self.log_after(txn, a.log_first)?
            };
        }
        self.save_attr(txn, &a)
    }

    /// The first change number above `n` (the log is not empty past it).
    fn log_after(&self, txn: &mut Txn<'_>, n: u64) -> Result<u64> {
        let t: Chunks = self.tree(txn, SLOT_LOG)?;
        let next = t.range(txn, ChunkKey::first(n + 1)..)?.next().transpose()?;
        Ok(next.map_or(0, |(k, _)| k.id))
    }

    /// Set how many records the log keeps (0 for no limit) and trim to
    /// it.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn set_log_max(&self, txn: &mut Txn<'_>, max: u64) -> Result<()> {
        let mut a = self.attr(txn)?;
        a.log_max = max;
        self.save_attr(txn, &a)?;
        if max > 0 {
            self.log_trim(txn, max)?;
        }
        Ok(())
    }
}
