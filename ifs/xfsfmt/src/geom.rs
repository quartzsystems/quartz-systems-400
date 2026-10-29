// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where everything goes: allocation groups, the log, the root inode
//! chunk, the free space. The placement follows mkfs.xfs so that
//! `xfs_repair`'s idea of where mkfs puts the root inode holds.

use crate::Error;

/// Filesystem block size.
pub const BLOCK_SIZE: u32 = 4096;
/// log2 of the block size.
pub const BLOCK_LOG: u8 = 12;
/// Inode size.
pub const INODE_SIZE: u32 = 512;
/// log2 of the inode size.
pub const INODE_LOG: u8 = 9;
/// Inodes per block.
pub const INOPB: u32 = BLOCK_SIZE / INODE_SIZE;
/// log2 of inodes per block.
pub const INOPB_LOG: u8 = 3;
/// Inodes in a chunk.
pub const CHUNK_INODES: u32 = 64;
/// Blocks in a chunk.
pub const CHUNK_BLOCKS: u32 = CHUNK_INODES / INOPB;
/// Inode chunk alignment in blocks (a 16 KiB cluster).
pub const INO_ALIGN: u32 = 4;
/// Blocks put on each AG free list at format time, as mkfs does.
pub const AGFL_PREFILL: u32 = 4;
/// Smallest AG: 16 MiB.
const MIN_AG_BLOCKS: u32 = 4096;
/// Largest AG: 1 TiB.
const MAX_AG_BLOCKS: u64 = 1 << 28;
/// Disk blocks per filesystem block.
const BB_PER_BLOCK: u64 = (BLOCK_SIZE / 512) as u64;
const MIB: u64 = 1024 * 1024;

/// Smallest device: four 16 MiB AGs.
pub const MIN_BYTES: u64 = 4 * MIN_AG_BLOCKS as u64 * BLOCK_SIZE as u64;
/// Largest device this formatter lays out: 64 TiB.
pub const MAX_BYTES: u64 = 64 << 40;

/// The geometry of one filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// Sector size, 512 or 4096. The log uses the same.
    pub sector_size: u32,
    /// Data blocks: `ag_count * ag_blocks`.
    pub dblocks: u64,
    /// Allocation groups.
    pub ag_count: u32,
    /// Blocks per AG (every AG is the same size).
    pub ag_blocks: u32,
    /// `ceil(log2(ag_blocks))`, the AG part of block and inode numbers.
    pub ag_blk_log: u8,
    /// The AG holding the internal log.
    pub log_ag: u32,
    /// The log's first block in that AG.
    pub log_agbno: u32,
    /// Log size in blocks.
    pub log_blocks: u32,
    /// AG 0 block of the first inode chunk (root, rt bitmap, rt summary).
    pub chunk_agbno: u32,
}

#[allow(clippy::similar_names)] // agno and agbno are the format's names.
impl Geometry {
    /// Lay out a filesystem on a device of `bytes`.
    pub fn for_bytes(bytes: u64, sector_size: u32) -> Result<Self, Error> {
        if sector_size != 512 && sector_size != 4096 {
            return Err(Error::SectorSize(sector_size));
        }
        if bytes < MIN_BYTES {
            return Err(Error::TooSmall {
                bytes,
                min: MIN_BYTES,
            });
        }
        if bytes > MAX_BYTES {
            return Err(Error::TooLarge {
                bytes,
                max: MAX_BYTES,
            });
        }
        let blocks = bytes / u64::from(BLOCK_SIZE);
        // Below 128 MiB: 16 MiB AGs. Above: four AGs, more once an AG
        // would pass 1 TiB. Trailing blocks short of a whole AG go unused.
        let (ag_count, ag_blocks) = if bytes < 128 * MIB {
            (blocks / u64::from(MIN_AG_BLOCKS), u64::from(MIN_AG_BLOCKS))
        } else {
            let count = blocks.div_ceil(MAX_AG_BLOCKS).max(4);
            (count, blocks / count)
        };
        let ag_count = u32::try_from(ag_count).expect("bounded by MAX_BYTES");
        let ag_blocks = u32::try_from(ag_blocks).expect("at most 2^28");
        let dblocks = u64::from(ag_count) * u64::from(ag_blocks);
        let ag_blk_log = u8::try_from(32 - (ag_blocks - 1).leading_zeros()).expect("< 32");

        let prealloc = prealloc_blocks(sector_size);
        let fs_bytes = dblocks * u64::from(BLOCK_SIZE);
        let floor = if fs_bytes >= 300 * MIB {
            64 * MIB
        } else {
            10 * MIB
        };
        let log_bytes = (fs_bytes / 2048).max(floor).min(1024 * MIB);
        let room = ag_blocks - prealloc - AGFL_PREFILL - 16;
        let log_blocks = u32::try_from(log_bytes / u64::from(BLOCK_SIZE))
            .expect("at most 1 GiB")
            .min(room);

        Ok(Self {
            sector_size,
            dblocks,
            ag_count,
            ag_blocks,
            ag_blk_log,
            log_ag: ag_count / 2,
            log_agbno: prealloc,
            log_blocks,
            chunk_agbno: (prealloc + AGFL_PREFILL).next_multiple_of(INO_ALIGN),
        })
    }

    /// Blocks at the head of every AG before free space: the headers
    /// and the four btree roots (bno, cnt, ino, fino).
    pub fn prealloc_blocks(&self) -> u32 {
        prealloc_blocks(self.sector_size)
    }

    /// log2 of the sector size.
    pub fn sector_log(&self) -> u8 {
        u8::try_from(self.sector_size.trailing_zeros()).expect("small")
    }

    /// The AG block holding the AG free list sector.
    pub fn agfl_block(&self) -> u32 {
        3 * self.sector_size / BLOCK_SIZE
    }

    /// Root btree blocks: (bno, cnt, ino, fino).
    pub fn roots(&self) -> [u32; 4] {
        let first = self.agfl_block() + 1;
        [first, first + 1, first + 2, first + 3]
    }

    /// Filesystem block number of an AG block.
    pub fn fsb(&self, agno: u32, agbno: u32) -> u64 {
        (u64::from(agno) << self.ag_blk_log) | u64::from(agbno)
    }

    /// 512-byte disk address of an AG block.
    pub fn daddr(&self, agno: u32, agbno: u32) -> u64 {
        (u64::from(agno) * u64::from(self.ag_blocks) + u64::from(agbno)) * BB_PER_BLOCK
    }

    /// Byte offset of an AG block.
    pub fn offset(&self, agno: u32, agbno: u32) -> u64 {
        self.daddr(agno, agbno) * 512
    }

    /// Filesystem block of the log start (`sb_logstart`).
    pub fn log_start(&self) -> u64 {
        self.fsb(self.log_ag, self.log_agbno)
    }

    /// Inode number of an AG 0 block's first inode.
    pub fn ino_at(&self, agbno: u32) -> u64 {
        u64::from(agbno) << INOPB_LOG
    }

    /// The root directory inode.
    pub fn root_ino(&self) -> u64 {
        self.ino_at(self.chunk_agbno)
    }

    /// The four blocks put on an AG's free list.
    pub fn agfl(&self, agno: u32) -> [u32; 4] {
        let first = if agno == self.log_ag {
            self.log_agbno + self.log_blocks
        } else {
            self.prealloc_blocks()
        };
        [first, first + 1, first + 2, first + 3]
    }

    /// Free extents of an AG as `(start, length)`, in block order.
    pub fn free_extents(&self, agno: u32) -> Vec<(u32, u32)> {
        let after_agfl = self.agfl(agno)[3] + 1;
        if agno == 0 {
            let mut out = Vec::new();
            if self.chunk_agbno > after_agfl {
                out.push((after_agfl, self.chunk_agbno - after_agfl));
            }
            let end = self.chunk_agbno + CHUNK_BLOCKS;
            out.push((end, self.ag_blocks - end));
            out
        } else {
            vec![(after_agfl, self.ag_blocks - after_agfl)]
        }
    }

    /// Free blocks in the free space btrees of an AG.
    pub fn free_blocks(&self, agno: u32) -> u32 {
        self.free_extents(agno).iter().map(|e| e.1).sum()
    }

    /// `sb_fdblocks`: free space plus the free lists.
    pub fn fdblocks(&self) -> u64 {
        (0..self.ag_count)
            .map(|a| u64::from(self.free_blocks(a) + AGFL_PREFILL))
            .sum()
    }
}

fn prealloc_blocks(sector_size: u32) -> u32 {
    3 * sector_size / BLOCK_SIZE + 1 + 4
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * MIB;

    #[test]
    fn small_filesystem_matches_mkfs() {
        // mkfs.xfs 6.18 -d agcount=4 -l size=2560b on 64 MiB.
        let g = Geometry::for_bytes(64 * MIB, 512).unwrap();
        assert_eq!((g.ag_count, g.ag_blocks, g.ag_blk_log), (4, 4096, 12));
        assert_eq!(g.log_start(), 8197);
        assert_eq!(g.log_blocks, 2560);
        assert_eq!(g.root_ino(), 96);
        assert_eq!(g.fdblocks(), 13796);
        assert_eq!(g.free_extents(0), vec![(9, 3), (20, 4076)]);
        assert_eq!(g.free_blocks(2), 1527);
    }

    #[test]
    fn four_k_sectors_match_mkfs() {
        let g = Geometry::for_bytes(64 * MIB, 4096).unwrap();
        assert_eq!(g.log_start(), 8200);
        assert_eq!(g.roots(), [4, 5, 6, 7]);
        assert_eq!(g.agfl(0), [8, 9, 10, 11]);
        assert_eq!(g.root_ino(), 96);
        assert_eq!(g.fdblocks(), 13784);
    }

    #[test]
    fn larger_filesystems() {
        let g = Geometry::for_bytes(GIB, 512).unwrap();
        assert_eq!((g.ag_count, g.ag_blocks, g.log_blocks), (4, 65536, 16384));
        assert_eq!(g.ag_blk_log, 16);
        let g = Geometry::for_bytes(256 * GIB + 4096 * 3, 512).unwrap();
        assert_eq!(g.ag_count, 4);
        assert_eq!(g.log_blocks, 32768);
        assert_eq!(g.dblocks, 4 * u64::from(g.ag_blocks));
        let g = Geometry::for_bytes(5 << 40, 4096).unwrap();
        assert_eq!(g.ag_count, 5);
        assert!(Geometry::for_bytes(MIN_BYTES - 1, 512).is_err());
        assert!(Geometry::for_bytes(GIB, 1024).is_err());
    }

    #[test]
    fn odd_sizes_keep_whole_ags() {
        for bytes in [MIN_BYTES + 4095, 100 * MIB + 12345, 513 * MIB + 7] {
            let g = Geometry::for_bytes(bytes, 512).unwrap();
            assert!(g.dblocks * u64::from(BLOCK_SIZE) <= bytes);
            assert!(g.log_agbno + g.log_blocks + AGFL_PREFILL < g.ag_blocks);
            assert_eq!(g.chunk_agbno % INO_ALIGN, 0);
        }
    }
}
