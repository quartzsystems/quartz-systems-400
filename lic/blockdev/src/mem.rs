// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The in-memory backend. It models a device with a volatile write cache:
//! a write is visible to reads at once but reaches the "platter" only on
//! `flush`. Faults can be injected to simulate a failing disk, a torn
//! sector write and a power pull.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::rng::Rng;
use crate::{BLOCK_SIZE, BlockDev, BlockError, Result, check_range};

/// Operation counters, for tests that assert on I/O behaviour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemStats {
    /// Block reads (calls, not blocks).
    pub reads: u64,
    /// Block writes (calls, not blocks), including ones that faulted.
    pub writes: u64,
    /// Flushes.
    pub flushes: u64,
    /// Blocks discarded.
    pub discarded: u64,
}

struct Inner {
    /// What has reached the platter.
    flushed: Vec<u8>,
    /// Blocks written since the last flush, by LBA.
    cache: HashMap<u64, Vec<u8>>,
    stats: MemStats,
    /// Every write after this one fails.
    fail_after: Option<u64>,
    /// This write persists only a prefix.
    tear_at: Option<u64>,
    rng: Rng,
}

/// A `Vec`-backed block device with fault injection.
pub struct MemBlockDev {
    blocks: u64,
    inner: Mutex<Inner>,
}

impl MemBlockDev {
    /// A zeroed device of `blocks` blocks with a fixed fault seed.
    #[must_use]
    pub fn new(blocks: u64) -> Self {
        Self::with_seed(blocks, 1)
    }

    /// A zeroed device whose tear points come from `seed`.
    #[must_use]
    pub fn with_seed(blocks: u64, seed: u64) -> Self {
        let bytes = usize::try_from(blocks * u64::from(BLOCK_SIZE)).expect("device fits in memory");
        Self {
            blocks,
            inner: Mutex::new(Inner {
                flushed: vec![0; bytes],
                cache: HashMap::new(),
                stats: MemStats::default(),
                fail_after: None,
                tear_at: None,
                rng: Rng::new(seed),
            }),
        }
    }

    /// Every write after the `n`th (counting from the start of the
    /// device's life, 1-based) returns [`BlockError::Injected`]. `None`
    /// clears the fault.
    pub fn fail_after(&self, n: Option<u64>) {
        self.lock().fail_after = n;
    }

    /// The `n`th write (1-based, lifetime count) persists only its first
    /// K bytes, K chosen from the seed, and reports success. `None` clears
    /// the fault.
    pub fn tear_at(&self, n: Option<u64>) {
        self.lock().tear_at = n;
    }

    /// Forget every write since the last flush, like a power pull on a
    /// device with a write cache.
    pub fn drop_unflushed(&self) {
        self.lock().cache.clear();
    }

    /// Number of blocks written but not yet flushed.
    #[must_use]
    pub fn unflushed(&self) -> usize {
        self.lock().cache.len()
    }

    /// The counters so far.
    #[must_use]
    pub fn stats(&self) -> MemStats {
        self.lock().stats
    }

    /// A snapshot of the flushed contents, for tests that compare images.
    #[must_use]
    pub fn flushed_image(&self) -> Vec<u8> {
        self.lock().flushed.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Inner {
    /// Store `data` for block `lba` in the cache.
    fn put(&mut self, lba: u64, data: &[u8]) {
        self.cache.insert(lba, data.to_vec());
    }

    /// Current contents of block `lba`: cached if written, else flushed.
    fn get(&self, lba: u64) -> &[u8] {
        if let Some(b) = self.cache.get(&lba) {
            return b;
        }
        let bs = BLOCK_SIZE as usize;
        let at = usize::try_from(lba).expect("lba fits") * bs;
        &self.flushed[at..at + bs]
    }
}

impl BlockDev for MemBlockDev {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let bs = BLOCK_SIZE as usize;
        let mut g = self.lock();
        g.stats.reads += 1;
        for i in 0..count {
            let dst = usize::try_from(i).expect("count fits") * bs;
            buf[dst..dst + bs].copy_from_slice(g.get(lba + i));
        }
        Ok(())
    }

    fn write(&self, lba: u64, buf: &[u8]) -> Result<()> {
        let count = check_range(self.blocks, lba, buf.len())?;
        let bs = BLOCK_SIZE as usize;
        let mut g = self.lock();
        g.stats.writes += 1;
        let n = g.stats.writes;
        if g.fail_after.is_some_and(|f| n > f) {
            return Err(BlockError::Injected {
                kind: "fail_after",
                write: n,
            });
        }
        // A torn write keeps a prefix of the buffer; the rest of the
        // affected block keeps its old bytes, later blocks are untouched.
        let keep = if g.tear_at == Some(n) {
            let k = g.rng.below(buf.len() as u64);
            usize::try_from(k).expect("len fits")
        } else {
            buf.len()
        };
        for i in 0..count {
            let src = usize::try_from(i).expect("count fits") * bs;
            if src >= keep {
                break;
            }
            let mut block = g.get(lba + i).to_vec();
            let n = (keep - src).min(bs);
            block[..n].copy_from_slice(&buf[src..src + n]);
            g.put(lba + i, &block);
        }
        Ok(())
    }

    fn flush(&self) -> Result<()> {
        let bs = BLOCK_SIZE as usize;
        let mut g = self.lock();
        g.stats.flushes += 1;
        let cache = std::mem::take(&mut g.cache);
        for (lba, block) in cache {
            let at = usize::try_from(lba).expect("lba fits") * bs;
            g.flushed[at..at + bs].copy_from_slice(&block);
        }
        Ok(())
    }

    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let len = usize::try_from(count * u64::from(BLOCK_SIZE)).expect("count fits");
        check_range(self.blocks, lba, len)?;
        let mut g = self.lock();
        g.stats.discarded += count;
        // Discarded blocks read as zero, at once and after a flush.
        let zero = vec![0u8; BLOCK_SIZE as usize];
        for i in 0..count {
            g.put(lba + i, &zero);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(fill: u8) -> Vec<u8> {
        vec![fill; 4096]
    }

    #[test]
    fn round_trip_and_bounds() {
        let d = MemBlockDev::new(4);
        d.write(1, &block(0xAB)).unwrap();
        let mut out = block(0);
        d.read(1, &mut out).unwrap();
        assert_eq!(out, block(0xAB));
        assert!(matches!(
            d.read(4, &mut out),
            Err(BlockError::OutOfRange { .. })
        ));
        assert!(matches!(
            d.write(0, &[1, 2, 3]),
            Err(BlockError::Unaligned { len: 3 })
        ));
        let mut two = vec![0u8; 8192];
        d.read(0, &mut two).unwrap();
        assert_eq!(&two[4096..], &block(0xAB)[..]);
        assert_eq!(d.stats().reads, 2);
    }

    #[test]
    fn unflushed_writes_can_be_dropped() {
        let d = MemBlockDev::new(2);
        d.write(0, &block(1)).unwrap();
        d.flush().unwrap();
        d.write(0, &block(2)).unwrap();
        d.write(1, &block(3)).unwrap();
        assert_eq!(d.unflushed(), 2);
        let mut out = block(0);
        d.read(0, &mut out).unwrap();
        assert_eq!(out, block(2), "unflushed writes are readable");
        d.drop_unflushed();
        d.read(0, &mut out).unwrap();
        assert_eq!(out, block(1), "back to the flushed contents");
        d.read(1, &mut out).unwrap();
        assert_eq!(out, block(0));
        assert_eq!(d.unflushed(), 0);
    }

    #[test]
    fn fail_after_counts_lifetime_writes() {
        let d = MemBlockDev::new(2);
        d.write(0, &block(1)).unwrap();
        d.fail_after(Some(2));
        d.write(0, &block(2)).unwrap();
        let e = d.write(0, &block(3)).unwrap_err();
        assert!(matches!(
            e,
            BlockError::Injected {
                kind: "fail_after",
                write: 3
            }
        ));
        d.fail_after(None);
        d.write(0, &block(4)).unwrap();
        let mut out = block(0);
        d.read(0, &mut out).unwrap();
        assert_eq!(out, block(4));
    }

    #[test]
    fn torn_write_keeps_a_prefix() {
        let d = MemBlockDev::with_seed(2, 7);
        d.write(0, &block(0x11)).unwrap();
        d.write(1, &block(0x11)).unwrap();
        d.flush().unwrap();
        d.tear_at(Some(3));
        let mut buf = block(0x22);
        buf.extend_from_slice(&block(0x33));
        d.write(0, &buf).unwrap();
        let mut out = vec![0u8; 8192];
        d.read(0, &mut out).unwrap();
        let kept = out.iter().take_while(|&&b| b != 0x11).count();
        assert!(
            kept < 8192,
            "a torn write persists less than the whole buffer"
        );
        assert_eq!(&out[..kept], &buf[..kept]);
        assert!(out[kept..].iter().all(|&b| b == 0x11));
        // Same seed, same tear point.
        let e = MemBlockDev::with_seed(2, 7);
        e.write(0, &block(0x11)).unwrap();
        e.write(1, &block(0x11)).unwrap();
        e.tear_at(Some(3));
        e.write(0, &buf).unwrap();
        let mut again = vec![0u8; 8192];
        e.read(0, &mut again).unwrap();
        assert_eq!(out, again);
    }

    #[test]
    fn discard_zeroes() {
        let d = MemBlockDev::new(3);
        d.write(1, &block(9)).unwrap();
        d.flush().unwrap();
        d.discard(1, 2).unwrap();
        let mut out = block(1);
        d.read(1, &mut out).unwrap();
        assert_eq!(out, block(0));
        assert!(matches!(
            d.discard(2, 2),
            Err(BlockError::OutOfRange { .. })
        ));
        assert_eq!(d.stats().discarded, 2);
    }
}
