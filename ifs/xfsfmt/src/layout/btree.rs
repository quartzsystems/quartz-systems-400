// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Short-form (AG-relative) v5 btree blocks: the free space trees by
//! block and by size, and the inode and free inode trees. A new
//! filesystem has one leaf per tree.

use super::{Buf, NULLAGBLOCK};
use crate::geom::BLOCK_SIZE;

/// Header field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod off {
    pub const MAGIC: usize = 0;
    pub const LEVEL: usize = 4;
    pub const NUMRECS: usize = 6;
    pub const LEFTSIB: usize = 8;
    pub const RIGHTSIB: usize = 12;
    pub const BLKNO: usize = 16;
    pub const LSN: usize = 24;
    pub const UUID: usize = 32;
    pub const OWNER: usize = 48;
    pub const CRC: usize = 52;
    /// First record.
    pub const RECS: usize = 56;
}

/// "AB3B", free space by block.
pub const BNO_MAGIC: u32 = 0x4142_3342;
/// "AB3C", free space by size.
pub const CNT_MAGIC: u32 = 0x4142_3343;
/// "IAB3", inodes.
pub const INO_MAGIC: u32 = 0x4941_4233;
/// "FIB3", chunks with free inodes.
pub const FINO_MAGIC: u32 = 0x4649_4233;

/// An inode chunk record: first AG inode, free count, free mask.
#[derive(Debug, Clone, Copy)]
pub struct InoRec {
    /// First AG inode of the chunk.
    pub start: u32,
    /// Free inodes in it.
    pub free_count: u32,
    /// One bit per inode, set when free.
    pub free: u64,
}

/// Where a block is and whose: its disk address, the filesystem UUID, its AG.
pub type Place<'a> = (u64, &'a [u8; 16], u32);

/// One block's header and the room for its records.
fn leaf(magic: u32, numrecs: usize, (daddr, uuid, agno): Place<'_>) -> Buf {
    let mut b = Buf::new(BLOCK_SIZE as usize);
    b.u32(off::MAGIC, magic);
    b.u16(off::LEVEL, 0);
    b.u16(off::NUMRECS, u16::try_from(numrecs).expect("one leaf"));
    b.u32(off::LEFTSIB, NULLAGBLOCK);
    b.u32(off::RIGHTSIB, NULLAGBLOCK);
    b.u64(off::BLKNO, daddr);
    b.bytes(off::UUID, uuid);
    b.u32(off::OWNER, agno);
    b
}

/// A free space leaf with `extents` in the order given.
pub fn alloc_leaf(magic: u32, extents: &[(u32, u32)], at: Place<'_>) -> Vec<u8> {
    let mut b = leaf(magic, extents.len(), at);
    for (i, (start, len)) in extents.iter().enumerate() {
        b.u32(off::RECS + 8 * i, *start);
        b.u32(off::RECS + 8 * i + 4, *len);
    }
    b.sealed(off::CRC)
}

/// An inode tree leaf.
pub fn ino_leaf(magic: u32, recs: &[InoRec], at: Place<'_>) -> Vec<u8> {
    let mut b = leaf(magic, recs.len(), at);
    for (i, r) in recs.iter().enumerate() {
        b.u32(off::RECS + 16 * i, r.start);
        b.u32(off::RECS + 16 * i + 4, r.free_count);
        b.u64(off::RECS + 16 * i + 8, r.free);
    }
    b.sealed(off::CRC)
}
