// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Block devices for the object store.
//!
//! One trait, [`BlockDev`], and two backends: [`LinuxBlockDev`] for a real
//! partition or disk opened with `O_DIRECT`, and [`MemBlockDev`], a
//! `Vec`-backed device with fault injection that every `store` test runs
//! on. Every transfer is a whole number of 4 KiB blocks.

#![deny(missing_docs)]

mod aligned;
mod error;
mod mem;
mod rng;

#[cfg(target_os = "linux")]
mod linux;

pub use aligned::AlignedBuf;
pub use error::{BlockError, Result};
pub use mem::{MemBlockDev, MemStats};

#[cfg(target_os = "linux")]
pub use linux::LinuxBlockDev;

/// The only block size in Phase 2.
pub const BLOCK_SIZE: u32 = 4096;

/// A device addressed in whole blocks. `lba` is the block number; buffers
/// are a multiple of [`BLOCK_SIZE`] bytes and transfer that many
/// consecutive blocks.
pub trait BlockDev: Send + Sync {
    /// Bytes per block. Always [`BLOCK_SIZE`] in Phase 2.
    fn block_size(&self) -> u32;

    /// Number of blocks on the device.
    fn blocks(&self) -> u64;

    /// Read `buf.len() / block_size()` blocks starting at `lba`.
    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()>;

    /// Write `buf.len() / block_size()` blocks starting at `lba`. The data
    /// is durable only after [`flush`](Self::flush).
    fn write(&self, lba: u64, buf: &[u8]) -> Result<()>;

    /// Make every earlier write durable (`fdatasync` or a cache barrier).
    fn flush(&self) -> Result<()>;

    /// Tell the device `count` blocks from `lba` are unused. Advisory.
    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        let _ = (lba, count);
        Ok(())
    }
}

/// Check a transfer against the block size and the device size: the
/// buffer must be a non-empty multiple of the block size and the range
/// must lie on the device. Returns the block count.
pub fn check_range(blocks: u64, lba: u64, len: usize) -> Result<u64> {
    let bs = BLOCK_SIZE as usize;
    if len == 0 || !len.is_multiple_of(bs) {
        return Err(BlockError::Unaligned { len });
    }
    let count = (len / bs) as u64;
    if lba.checked_add(count).is_none_or(|end| end > blocks) {
        return Err(BlockError::OutOfRange { lba, count, blocks });
    }
    Ok(count)
}

impl<T: BlockDev + ?Sized> BlockDev for Box<T> {
    fn block_size(&self) -> u32 {
        (**self).block_size()
    }
    fn blocks(&self) -> u64 {
        (**self).blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        (**self).read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> Result<()> {
        (**self).write(lba, buf)
    }
    fn flush(&self) -> Result<()> {
        (**self).flush()
    }
    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        (**self).discard(lba, count)
    }
}

impl<T: BlockDev + ?Sized> BlockDev for std::sync::Arc<T> {
    fn block_size(&self) -> u32 {
        (**self).block_size()
    }
    fn blocks(&self) -> u64 {
        (**self).blocks()
    }
    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        (**self).read(lba, buf)
    }
    fn write(&self, lba: u64, buf: &[u8]) -> Result<()> {
        (**self).write(lba, buf)
    }
    fn flush(&self) -> Result<()> {
        (**self).flush()
    }
    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        (**self).discard(lba, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_checks() {
        assert_eq!(check_range(10, 0, 4096).unwrap(), 1);
        assert_eq!(check_range(10, 8, 8192).unwrap(), 2);
        assert!(matches!(
            check_range(10, 9, 8192),
            Err(BlockError::OutOfRange {
                lba: 9,
                count: 2,
                blocks: 10
            })
        ));
        assert!(matches!(
            check_range(10, 0, 100),
            Err(BlockError::Unaligned { len: 100 })
        ));
        assert!(matches!(
            check_range(10, 0, 0),
            Err(BlockError::Unaligned { len: 0 })
        ));
        assert!(matches!(
            check_range(10, u64::MAX, 4096),
            Err(BlockError::OutOfRange { .. })
        ));
    }
}
