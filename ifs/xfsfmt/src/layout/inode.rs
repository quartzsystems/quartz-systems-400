// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version 3 inodes (`xfs_dinode`): the root directory in short form,
//! the two empty realtime inodes, and free inodes.

use super::{Buf, NULLAGINO};
use crate::Timestamp;
use crate::geom::INODE_SIZE;

/// Field offsets.
#[allow(dead_code)] // The whole table; the tests hold it against the format.
pub mod off {
    pub const MAGIC: usize = 0;
    pub const MODE: usize = 2;
    pub const VERSION: usize = 4;
    pub const FORMAT: usize = 5;
    pub const UID: usize = 8;
    pub const GID: usize = 12;
    pub const NLINK: usize = 16;
    pub const ATIME: usize = 32;
    pub const MTIME: usize = 40;
    pub const CTIME: usize = 48;
    pub const SIZE: usize = 56;
    pub const NBLOCKS: usize = 64;
    pub const NEXTENTS: usize = 76;
    pub const FORKOFF: usize = 82;
    pub const AFORMAT: usize = 83;
    pub const FLAGS: usize = 90;
    pub const GEN: usize = 92;
    pub const NEXT_UNLINKED: usize = 96;
    pub const CRC: usize = 100;
    pub const CHANGECOUNT: usize = 104;
    pub const LSN: usize = 112;
    pub const FLAGS2: usize = 120;
    pub const CRTIME: usize = 144;
    pub const INO: usize = 152;
    pub const UUID: usize = 160;
    /// The data fork.
    pub const DATA: usize = 176;
}

/// "IN".
pub const MAGIC: u16 = 0x494e;
const FMT_LOCAL: u8 = 1;
const FMT_EXTENTS: u8 = 2;
const S_IFDIR: u16 = 0o040_000;
const S_IFREG: u16 = 0o100_000;
/// `XFS_DIFLAG_NEWRTBM`: the bitmap inode's flag.
const DIFLAG_NEWRTBM: u16 = 1 << 2;
/// `XFS_DIFLAG2_BIGTIME`.
const DIFLAG2_BIGTIME: u64 = 1 << 3;
/// Bytes of an empty short-form directory: count, i8count, 4-byte parent.
pub const SF_DIR_EMPTY: u64 = 6;

/// Which inode to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The root directory, empty, mode 0755.
    Root,
    /// The realtime bitmap (no realtime volume, so empty).
    RtBitmap,
    /// The realtime summary (empty).
    RtSummary,
    /// An unallocated inode in an allocated chunk.
    Free,
}

/// A bigtime timestamp: nanoseconds since 1901-12-13T20:45:52Z.
pub fn bigtime(t: Timestamp) -> u64 {
    let secs = u64::try_from(t.secs + (1i64 << 31)).unwrap_or(0);
    secs * 1_000_000_000 + u64::from(t.nsecs)
}

/// Inode `ino` of `kind`. `time` stamps an allocated inode.
pub fn build(kind: Kind, ino: u64, root: u64, time: Timestamp, uuid: &[u8; 16]) -> Vec<u8> {
    let mut b = Buf::new(INODE_SIZE as usize);
    b.u16(off::MAGIC, MAGIC);
    b.u8(off::VERSION, 3);
    b.u32(off::NEXT_UNLINKED, NULLAGINO);
    b.u64(off::INO, ino);
    b.bytes(off::UUID, uuid);
    if kind != Kind::Free {
        let now = bigtime(time);
        let epoch = bigtime(Timestamp { secs: 0, nsecs: 0 });
        let (mode, format, nlink) = match kind {
            Kind::Root => (S_IFDIR | 0o755, FMT_LOCAL, 2),
            _ => (S_IFREG, FMT_EXTENTS, 1),
        };
        b.u16(off::MODE, mode);
        b.u8(off::FORMAT, format);
        b.u32(off::NLINK, nlink);
        // The bitmap's atime is the realtime allocator's rotor seed.
        b.u64(off::ATIME, if kind == Kind::RtBitmap { epoch } else { now });
        b.u64(off::MTIME, now);
        b.u64(off::CTIME, now);
        b.u64(off::CRTIME, now);
        b.u8(off::AFORMAT, FMT_EXTENTS);
        b.u64(off::CHANGECOUNT, 2);
        b.u64(off::FLAGS2, DIFLAG2_BIGTIME);
        if kind == Kind::RtBitmap {
            b.u16(off::FLAGS, DIFLAG_NEWRTBM);
        }
        if kind == Kind::Root {
            b.u64(off::SIZE, SF_DIR_EMPTY);
            // Short-form header: no entries, parent is itself.
            b.u32(off::DATA + 2, u32::try_from(root).expect("root below 2^32"));
        }
    }
    b.sealed(off::CRC)
}
