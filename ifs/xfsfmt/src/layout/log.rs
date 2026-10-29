// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The clean internal log: one cycle-1 record holding an unmount
//! record at the log's first block, zeros after it. The kernel finds
//! the head there, sees the unmount record and skips recovery.

use super::Buf;

/// Record header field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod off {
    pub const MAGIC: usize = 0;
    pub const CYCLE: usize = 4;
    pub const VERSION: usize = 8;
    pub const LEN: usize = 12;
    pub const LSN: usize = 16;
    pub const TAIL_LSN: usize = 24;
    pub const CRC: usize = 32;
    pub const PREV_BLOCK: usize = 36;
    pub const NUM_LOGOPS: usize = 40;
    pub const CYCLE_DATA: usize = 44;
    pub const FMT: usize = 300;
    pub const UUID: usize = 304;
    pub const SIZE: usize = 320;
}

/// Log record header magic.
pub const MAGIC: u32 = 0xfeed_babe;
/// Basic block.
const BB: usize = 512;
/// The cycle a new log starts in.
const CYCLE: u32 = 1;
/// `XLOG_FMT_LINUX_LE`.
const FMT_LINUX_LE: u32 = 1;
/// In-core log buffer size the header advertises.
const BIG_RECORD: u32 = 32 * 1024;
/// Transaction ID mkfs marks its record with.
const USERSPACE_TID: u32 = 0xb0c0_d0d0;
/// `XFS_LOG` client.
const CLIENT_LOG: u8 = 0xaa;
/// `XLOG_UNMOUNT_TRANS`.
const UNMOUNT_TRANS: u8 = 0x20;
/// `XLOG_UNMOUNT_TYPE`, stored in host (little-endian) order.
const UNMOUNT_TYPE: u16 = 0x556e;

/// The record for a log with `sector_size` sectors: a header block and
/// as many data blocks as fill the log stripe unit (at least one).
pub fn clean_record(sector_size: u32, uuid: &[u8; 16]) -> Vec<u8> {
    let bbs = (sector_size as usize / BB).max(2);
    let mut b = Buf::new(bbs * BB);
    let lsn = u64::from(CYCLE) << 32;
    b.u32(off::MAGIC, MAGIC);
    b.u32(off::CYCLE, CYCLE);
    b.u32(off::VERSION, 2);
    b.u32(off::LEN, u32::try_from((bbs - 1) * BB).expect("small"));
    b.u64(off::LSN, lsn);
    b.u64(off::TAIL_LSN, lsn);
    b.u32(off::PREV_BLOCK, u32::MAX);
    b.u32(off::NUM_LOGOPS, 1);
    b.u32(off::FMT, FMT_LINUX_LE);
    b.bytes(off::UUID, uuid);
    b.u32(off::SIZE, BIG_RECORD);

    // The unmount op, its first word packed into the header's cycle
    // data and replaced by the cycle number, as every data block's is.
    b.u32(off::CYCLE_DATA, USERSPACE_TID);
    let op = BB;
    b.u32(op + 4, 8);
    b.u8(op + 8, CLIENT_LOG);
    b.u8(op + 9, UNMOUNT_TRANS);
    b.bytes(op + 12, &UNMOUNT_TYPE.to_le_bytes());
    for blk in 1..bbs {
        b.u32(blk * BB, CYCLE);
    }
    // mkfs leaves the record CRC zero; the kernel accepts it for a
    // clean log, which it never replays.
    b.0
}
