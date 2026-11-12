// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A run of blocks of a device as a device: what a caller puts under a
//! set to keep blocks of each disk for a label of its own.

use blockdev::{BLOCK_SIZE, BlockDev, BlockError, Result, check_range};

/// Blocks `start..start + blocks` of a device.
pub struct Window<D> {
    dev: D,
    start: u64,
    blocks: u64,
}

impl<D: BlockDev> Window<D> {
    /// The run, which must lie on the device.
    pub fn new(dev: D, start: u64, blocks: u64) -> Result<Self> {
        if start
            .checked_add(blocks)
            .is_none_or(|end| end > dev.blocks())
        {
            return Err(BlockError::OutOfRange {
                lba: start,
                count: blocks,
                blocks: dev.blocks(),
            });
        }
        Ok(Self { dev, start, blocks })
    }

    /// The device, whole.
    pub fn into_inner(self) -> D {
        self.dev
    }
}

impl<D: BlockDev> BlockDev for Window<D> {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        check_range(self.blocks, lba, buf.len())?;
        self.dev.read(self.start + lba, buf)
    }

    fn write(&self, lba: u64, buf: &[u8]) -> Result<()> {
        check_range(self.blocks, lba, buf.len())?;
        self.dev.write(self.start + lba, buf)
    }

    fn flush(&self) -> Result<()> {
        self.dev.flush()
    }

    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        if lba.checked_add(count).is_none_or(|end| end > self.blocks) {
            return Err(BlockError::OutOfRange {
                lba,
                count,
                blocks: self.blocks,
            });
        }
        self.dev.discard(self.start + lba, count)
    }
}

#[cfg(test)]
mod tests {
    use blockdev::{AlignedBuf, MemBlockDev};

    use super::*;

    #[test]
    fn a_window_leaves_the_rest_alone() {
        let w = Window::new(MemBlockDev::new(16), 4, 8).unwrap();
        let mut b = AlignedBuf::new(1);
        b.fill(7);
        w.write(0, &b).unwrap();
        assert!(w.write(8, &b).is_err());
        let dev = w.into_inner();
        let mut got = AlignedBuf::new(1);
        dev.read(4, &mut got).unwrap();
        assert_eq!(got[0], 7);
        dev.read(0, &mut got).unwrap();
        assert_eq!(got[0], 0);
        assert!(Window::new(dev, 10, 8).is_err());
    }
}
