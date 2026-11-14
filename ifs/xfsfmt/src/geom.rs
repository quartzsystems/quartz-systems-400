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
/// Blocks put on each AG free list at format time, as mkfs does: two
/// per free space btree (`xfs_alloc_min_freelist` at level 1).
pub const AGFL_PREFILL: u32 = 4;
/// The same with the reverse-mapping btree: two more for it.
pub const AGFL_PREFILL_RMAP: u32 = 6;
/// Smallest AG: 16 MiB.
const MIN_AG_BLOCKS: u32 = 4096;
/// Largest AG: 1 TiB.
const MAX_AG_BLOCKS: u64 = 1 << 28;
/// Disk blocks per filesystem block.
const BB_PER_BLOCK: u64 = (BLOCK_SIZE / 512) as u64;
const MIB: u64 = 1024 * 1024;
/// The log below 300 MiB with reflink. The kernel wants 2928 to 3273
/// blocks there (above the 10 MiB floor); 14 MiB keeps a margin and
/// still fits a 16 MiB AG.
const REFLINK_SMALL_LOG: u64 = 14 * MIB;

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
    /// Reflink with the reverse-mapping btree: two more roots in every
    /// AG (rmap, refcount) and a longer free list.
    pub reflink: bool,
}

/// An rmap owner that is not an inode (`XFS_RMAP_OWN_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RmapOwner {
    /// The AG headers.
    Fs,
    /// The internal log.
    Log,
    /// Free space and rmap btree blocks, and the free list's blocks.
    Ag,
    /// Inode btree blocks.
    Inobt,
    /// Inode chunks.
    Inodes,
    /// Refcount btree blocks.
    Refc,
}

impl RmapOwner {
    /// The on-disk owner value.
    pub fn value(self) -> u64 {
        let n: i64 = match self {
            Self::Fs => -3,
            Self::Log => -4,
            Self::Ag => -5,
            Self::Inobt => -6,
            Self::Inodes => -7,
            Self::Refc => -8,
        };
        n.cast_unsigned()
    }
}

#[allow(clippy::similar_names)] // agno and agbno are the format's names.
impl Geometry {
    /// Lay out a filesystem on a device of `bytes`.
    pub fn for_bytes(bytes: u64, sector_size: u32) -> Result<Self, Error> {
        Self::for_bytes_with(bytes, sector_size, None)
    }

    /// The same with the AG size fixed (16 MiB to 1 TiB, and the
    /// device at least one AG long), when `ag` is given.
    pub fn for_bytes_with(bytes: u64, sector_size: u32, ag: Option<u32>) -> Result<Self, Error> {
        Self::for_features(bytes, sector_size, ag, false)
    }

    /// The same with reflink and the reverse-mapping btree when
    /// `reflink` is set.
    pub fn for_features(
        bytes: u64,
        sector_size: u32,
        ag: Option<u32>,
        reflink: bool,
    ) -> Result<Self, Error> {
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
        let fixed = ag.map(|a| u64::from(a).clamp(u64::from(MIN_AG_BLOCKS), MAX_AG_BLOCKS));
        let (ag_count, ag_blocks) = if let Some(a) = fixed {
            if blocks < a {
                return Err(Error::TooSmall {
                    bytes,
                    min: a * u64::from(BLOCK_SIZE),
                });
            }
            (blocks / a, a)
        } else if bytes < 128 * MIB {
            (blocks / u64::from(MIN_AG_BLOCKS), u64::from(MIN_AG_BLOCKS))
        } else {
            let count = blocks.div_ceil(MAX_AG_BLOCKS).max(4);
            (count, blocks / count)
        };
        let ag_count = u32::try_from(ag_count).expect("bounded by MAX_BYTES");
        let ag_blocks = u32::try_from(ag_blocks).expect("at most 2^28");
        let dblocks = u64::from(ag_count) * u64::from(ag_blocks);
        let ag_blk_log = u8::try_from(32 - (ag_blocks - 1).leading_zeros()).expect("< 32");

        let prealloc = prealloc_blocks(sector_size, reflink);
        let agfl = agfl_prefill(reflink);
        let fs_bytes = dblocks * u64::from(BLOCK_SIZE);
        let floor = if fs_bytes >= 300 * MIB {
            64 * MIB
        } else if reflink {
            REFLINK_SMALL_LOG
        } else {
            10 * MIB
        };
        let log_bytes = (fs_bytes / 2048).max(floor).min(1024 * MIB);
        let room = ag_blocks - prealloc - agfl - 16;
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
            chunk_agbno: (prealloc + agfl).next_multiple_of(INO_ALIGN),
            reflink,
        })
    }

    /// Blocks at the head of every AG before free space: the headers
    /// and the btree roots (bno, cnt, ino, fino; rmap and refcount with
    /// reflink).
    pub fn prealloc_blocks(&self) -> u32 {
        prealloc_blocks(self.sector_size, self.reflink)
    }

    /// Blocks on each AG free list.
    pub fn agfl_count(&self) -> u32 {
        agfl_prefill(self.reflink)
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

    /// The rmap and refcount btree roots, with reflink.
    pub fn reflink_roots(&self) -> Option<[u32; 2]> {
        let after = self.roots()[3] + 1;
        self.reflink.then_some([after, after + 1])
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

    /// The first four blocks put on an AG's free list (all of them
    /// without reflink; [`Geometry::agfl_blocks`] has every one).
    pub fn agfl(&self, agno: u32) -> [u32; 4] {
        let first = self.agfl_blocks(agno).start;
        [first, first + 1, first + 2, first + 3]
    }

    /// Every block put on an AG's free list.
    pub fn agfl_blocks(&self, agno: u32) -> std::ops::Range<u32> {
        let first = if agno == self.log_ag {
            self.log_agbno + self.log_blocks
        } else {
            self.prealloc_blocks()
        };
        first..first + self.agfl_count()
    }

    /// Free extents of an AG as `(start, length)`, in block order.
    pub fn free_extents(&self, agno: u32) -> Vec<(u32, u32)> {
        let after_agfl = self.agfl_blocks(agno).end;
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
            .map(|a| u64::from(self.free_blocks(a) + self.agfl_count()))
            .sum()
    }

    /// The used space of an AG as rmap records `(start, length, owner)`,
    /// in block order, neighbours of one owner merged, as mkfs writes
    /// them into a new rmap btree.
    pub fn rmap_records(&self, agno: u32) -> Vec<(u32, u32, RmapOwner)> {
        let headers = self.agfl_block() + 1;
        let [bno, _, ino, _] = self.roots();
        let mut used = vec![
            (0, headers, RmapOwner::Fs),
            (bno, 2, RmapOwner::Ag),
            (ino, 2, RmapOwner::Inobt),
        ];
        if let Some([rmap, refc]) = self.reflink_roots() {
            used.push((rmap, 1, RmapOwner::Ag));
            used.push((refc, 1, RmapOwner::Refc));
        }
        if agno == self.log_ag {
            used.push((self.log_agbno, self.log_blocks, RmapOwner::Log));
        }
        let agfl = self.agfl_blocks(agno);
        used.push((agfl.start, agfl.end - agfl.start, RmapOwner::Ag));
        if agno == 0 {
            used.push((self.chunk_agbno, CHUNK_BLOCKS, RmapOwner::Inodes));
        }
        used.sort_by_key(|r| r.0);
        let mut out: Vec<(u32, u32, RmapOwner)> = Vec::with_capacity(used.len());
        for r in used {
            match out.last_mut() {
                Some(last) if last.2 == r.2 && last.0 + last.1 == r.0 => last.1 += r.1,
                _ => out.push(r),
            }
        }
        out
    }
}

fn prealloc_blocks(sector_size: u32, reflink: bool) -> u32 {
    3 * sector_size / BLOCK_SIZE + 1 + 4 + if reflink { 2 } else { 0 }
}

fn agfl_prefill(reflink: bool) -> u32 {
    if reflink {
        AGFL_PREFILL_RMAP
    } else {
        AGFL_PREFILL
    }
}

#[cfg(test)]
mod tests;
