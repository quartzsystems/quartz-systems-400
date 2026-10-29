// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The superblock (`xfs_dsb`), one sector at block 0 of every AG.

use super::Buf;
use crate::Geometry;
use crate::geom::{BLOCK_LOG, BLOCK_SIZE, INO_ALIGN, INODE_LOG, INODE_SIZE, INOPB, INOPB_LOG};

/// Field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod off {
    pub const MAGIC: usize = 0;
    pub const BLOCKSIZE: usize = 4;
    pub const DBLOCKS: usize = 8;
    pub const UUID: usize = 32;
    pub const LOGSTART: usize = 48;
    pub const ROOTINO: usize = 56;
    pub const RBMINO: usize = 64;
    pub const RSUMINO: usize = 72;
    pub const REXTSIZE: usize = 80;
    pub const AGBLOCKS: usize = 84;
    pub const AGCOUNT: usize = 88;
    pub const LOGBLOCKS: usize = 96;
    pub const VERSIONNUM: usize = 100;
    pub const SECTSIZE: usize = 102;
    pub const INODESIZE: usize = 104;
    pub const INOPBLOCK: usize = 106;
    pub const FNAME: usize = 108;
    pub const BLOCKLOG: usize = 120;
    pub const SECTLOG: usize = 121;
    pub const INODELOG: usize = 122;
    pub const INOPBLOG: usize = 123;
    pub const AGBLKLOG: usize = 124;
    pub const IMAX_PCT: usize = 127;
    pub const ICOUNT: usize = 128;
    pub const IFREE: usize = 136;
    pub const FDBLOCKS: usize = 144;
    pub const INOALIGNMT: usize = 180;
    pub const LOGSECTLOG: usize = 193;
    pub const LOGSECTSIZE: usize = 194;
    pub const LOGSUNIT: usize = 196;
    pub const FEATURES2: usize = 200;
    pub const BAD_FEATURES2: usize = 204;
    pub const FEATURES_RO_COMPAT: usize = 212;
    pub const FEATURES_INCOMPAT: usize = 216;
    pub const CRC: usize = 224;
    pub const META_UUID: usize = 248;
}

/// "XFSB".
pub const MAGIC: u32 = 0x5846_5342;
/// Version 5 with NLINK, ALIGN, LOGV2, EXTFLG, DIRV2 and MOREBITS.
const VERSION_V5: u16 = 0xb4a5;
/// SECTORBIT, for sectors other than 512 bytes.
const VERSION_SECTOR: u16 = 0x0800;
/// LAZYSBCOUNT, ATTR2, PROJID32, CRC.
const FEATURES2: u32 = 0x18a;
/// FINOBT, INOBTCNT.
pub const RO_COMPAT: u32 = 0x1 | 0x8;
/// FTYPE, BIGTIME.
pub const INCOMPAT: u32 = 0x1 | 0x8;

/// Inode counts the superblock carries.
pub struct Counts {
    /// Allocated inodes.
    pub icount: u64,
    /// Free inodes among them.
    pub ifree: u64,
}

/// Build the superblock sector.
pub fn build(g: &Geometry, uuid: &[u8; 16], label: &[u8], counts: &Counts) -> Vec<u8> {
    let mut b = Buf::new(g.sector_size as usize);
    let root = g.root_ino();
    b.u32(off::MAGIC, MAGIC);
    b.u32(off::BLOCKSIZE, BLOCK_SIZE);
    b.u64(off::DBLOCKS, g.dblocks);
    b.bytes(off::UUID, uuid);
    b.u64(off::LOGSTART, g.log_start());
    b.u64(off::ROOTINO, root);
    b.u64(off::RBMINO, root + 1);
    b.u64(off::RSUMINO, root + 2);
    b.u32(off::REXTSIZE, 1);
    b.u32(off::AGBLOCKS, g.ag_blocks);
    b.u32(off::AGCOUNT, g.ag_count);
    b.u32(off::LOGBLOCKS, g.log_blocks);
    let sector_bit = if g.sector_size == 512 {
        0
    } else {
        VERSION_SECTOR
    };
    b.u16(off::VERSIONNUM, VERSION_V5 | sector_bit);
    b.u16(
        off::SECTSIZE,
        u16::try_from(g.sector_size).expect("4096 at most"),
    );
    b.u16(off::INODESIZE, u16::try_from(INODE_SIZE).expect("512"));
    b.u16(off::INOPBLOCK, u16::try_from(INOPB).expect("8"));
    b.bytes(off::FNAME, label);
    b.u8(off::BLOCKLOG, BLOCK_LOG);
    b.u8(off::SECTLOG, g.sector_log());
    b.u8(off::INODELOG, INODE_LOG);
    b.u8(off::INOPBLOG, INOPB_LOG);
    b.u8(off::AGBLKLOG, g.ag_blk_log);
    b.u8(off::IMAX_PCT, 25);
    b.u64(off::ICOUNT, counts.icount);
    b.u64(off::IFREE, counts.ifree);
    b.u64(off::FDBLOCKS, g.fdblocks());
    b.u32(off::INOALIGNMT, INO_ALIGN);
    if g.sector_size == 512 {
        b.u32(off::LOGSUNIT, 1);
    } else {
        b.u8(off::LOGSECTLOG, g.sector_log());
        b.u16(
            off::LOGSECTSIZE,
            u16::try_from(g.sector_size).expect("4096"),
        );
        b.u32(off::LOGSUNIT, g.sector_size);
    }
    b.u32(off::FEATURES2, FEATURES2);
    b.u32(off::BAD_FEATURES2, FEATURES2);
    b.u32(off::FEATURES_RO_COMPAT, RO_COMPAT);
    b.u32(off::FEATURES_INCOMPAT, INCOMPAT);
    b.sealed(off::CRC)
}
