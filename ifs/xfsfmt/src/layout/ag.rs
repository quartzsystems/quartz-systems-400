// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The AG headers after the superblock: the free space header (AGF),
//! the inode header (AGI) and the free list (AGFL), one sector each.

use super::{Buf, NULLAGBLOCK, NULLAGINO};
use crate::Geometry;
use crate::geom::AGFL_PREFILL;

/// AGF field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod agf {
    pub const MAGIC: usize = 0;
    pub const VERSION: usize = 4;
    pub const SEQNO: usize = 8;
    pub const LENGTH: usize = 12;
    pub const BNO_ROOT: usize = 16;
    pub const CNT_ROOT: usize = 20;
    pub const BNO_LEVEL: usize = 28;
    pub const CNT_LEVEL: usize = 32;
    pub const FLFIRST: usize = 40;
    pub const FLLAST: usize = 44;
    pub const FLCOUNT: usize = 48;
    pub const FREEBLKS: usize = 52;
    pub const LONGEST: usize = 56;
    pub const UUID: usize = 64;
    pub const LSN: usize = 208;
    pub const CRC: usize = 216;
}

/// AGI field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod agi {
    pub const MAGIC: usize = 0;
    pub const VERSION: usize = 4;
    pub const SEQNO: usize = 8;
    pub const LENGTH: usize = 12;
    pub const COUNT: usize = 16;
    pub const ROOT: usize = 20;
    pub const LEVEL: usize = 24;
    pub const FREECOUNT: usize = 28;
    pub const NEWINO: usize = 32;
    pub const DIRINO: usize = 36;
    pub const UNLINKED: usize = 40;
    pub const UUID: usize = 296;
    pub const CRC: usize = 312;
    pub const LSN: usize = 320;
    pub const FREE_ROOT: usize = 328;
    pub const FREE_LEVEL: usize = 332;
    pub const IBLOCKS: usize = 336;
    pub const FBLOCKS: usize = 340;
}

/// AGFL field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod agfl {
    pub const MAGIC: usize = 0;
    pub const SEQNO: usize = 4;
    pub const UUID: usize = 8;
    pub const LSN: usize = 24;
    pub const CRC: usize = 32;
    pub const BNO: usize = 36;
}

/// "XAGF".
pub const AGF_MAGIC: u32 = 0x5841_4746;
/// "XAGI".
pub const AGI_MAGIC: u32 = 0x5841_4749;
/// "XAFL".
pub const AGFL_MAGIC: u32 = 0x5841_464c;
/// Free list slot of the first prefilled block (mkfs leaves slot 0 empty).
const FLFIRST: u32 = 1;

/// The free space header of AG `agno`.
pub fn build_agf(g: &Geometry, agno: u32, uuid: &[u8; 16]) -> Vec<u8> {
    let mut b = Buf::new(g.sector_size as usize);
    let [bno, cnt, _, _] = g.roots();
    let longest = g.free_extents(agno).iter().map(|e| e.1).max().unwrap_or(0);
    b.u32(agf::MAGIC, AGF_MAGIC);
    b.u32(agf::VERSION, 1);
    b.u32(agf::SEQNO, agno);
    b.u32(agf::LENGTH, g.ag_blocks);
    b.u32(agf::BNO_ROOT, bno);
    b.u32(agf::CNT_ROOT, cnt);
    b.u32(agf::BNO_LEVEL, 1);
    b.u32(agf::CNT_LEVEL, 1);
    b.u32(agf::FLFIRST, FLFIRST);
    b.u32(agf::FLLAST, FLFIRST + AGFL_PREFILL - 1);
    b.u32(agf::FLCOUNT, AGFL_PREFILL);
    b.u32(agf::FREEBLKS, g.free_blocks(agno));
    b.u32(agf::LONGEST, longest);
    b.bytes(agf::UUID, uuid);
    b.sealed(agf::CRC)
}

/// Inode state of an AG for its AGI.
pub struct Inodes {
    /// Allocated inodes.
    pub count: u32,
    /// Free inodes among them.
    pub free: u32,
    /// First inode of the newest chunk, if any.
    pub newino: Option<u32>,
}

/// The inode header of AG `agno`.
pub fn build_agi(g: &Geometry, agno: u32, uuid: &[u8; 16], inodes: &Inodes) -> Vec<u8> {
    let mut b = Buf::new(g.sector_size as usize);
    let [_, _, ino, fino] = g.roots();
    b.u32(agi::MAGIC, AGI_MAGIC);
    b.u32(agi::VERSION, 1);
    b.u32(agi::SEQNO, agno);
    b.u32(agi::LENGTH, g.ag_blocks);
    b.u32(agi::COUNT, inodes.count);
    b.u32(agi::ROOT, ino);
    b.u32(agi::LEVEL, 1);
    b.u32(agi::FREECOUNT, inodes.free);
    b.u32(agi::NEWINO, inodes.newino.unwrap_or(NULLAGINO));
    b.u32(agi::DIRINO, NULLAGINO);
    for i in 0..64 {
        b.u32(agi::UNLINKED + 4 * i, NULLAGINO);
    }
    b.bytes(agi::UUID, uuid);
    b.u32(agi::FREE_ROOT, fino);
    b.u32(agi::FREE_LEVEL, 1);
    b.u32(agi::IBLOCKS, 1);
    b.u32(agi::FBLOCKS, 1);
    b.sealed(agi::CRC)
}

/// The free list of AG `agno`, holding the prefilled blocks.
pub fn build_agfl(g: &Geometry, agno: u32, uuid: &[u8; 16]) -> Vec<u8> {
    let len = g.sector_size as usize;
    let mut b = Buf::new(len);
    b.u32(agfl::MAGIC, AGFL_MAGIC);
    b.u32(agfl::SEQNO, agno);
    b.bytes(agfl::UUID, uuid);
    for slot in 0..(len - agfl::BNO) / 4 {
        b.u32(agfl::BNO + 4 * slot, NULLAGBLOCK);
    }
    for (i, bno) in g.agfl(agno).into_iter().enumerate() {
        b.u32(agfl::BNO + 4 * (FLFIRST as usize + i), bno);
    }
    b.sealed(agfl::CRC)
}
