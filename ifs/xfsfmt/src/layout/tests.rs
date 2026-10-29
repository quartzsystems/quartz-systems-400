// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The structures against the XFS on-disk format: field offsets as
//! `xfs_format.h` and `xfs_log_format.h` place them, and whole sectors
//! against what `mkfs.xfs` 6.18 writes for the same geometry and UUID
//! (`-b size=4096 -s size=512 -m crc=1,finobt=1,rmapbt=0,reflink=0,
//! bigtime=1,inobtcount=1 -i sparse=0,size=512,nrext64=0 -n ftype=1
//! -d agcount=4 -l size=2560b -L QS400-IFS` on 64 MiB).

use super::{ag, btree, inode, log, sb};
use crate::{Geometry, Timestamp};

const UUID: [u8; 16] = [
    0x51, 0x53, 0x34, 0x30, 0, 0, 0x40, 0, 0x80, 0, 0, 0, 0, 0, 0, 5,
];

/// A sector from `(offset, hex)` rows; everything else zero.
fn sector(len: usize, rows: &[(usize, &str)]) -> Vec<u8> {
    let mut out = vec![0u8; len];
    for (at, hex) in rows {
        let digits: Vec<u8> = hex.bytes().filter(u8::is_ascii_hexdigit).collect();
        for (i, pair) in digits.chunks(2).enumerate() {
            let s = std::str::from_utf8(pair).unwrap();
            out[at + i] = u8::from_str_radix(s, 16).unwrap();
        }
    }
    out
}

fn geometry() -> Geometry {
    Geometry::for_bytes(64 * 1024 * 1024, 512).unwrap()
}

#[test]
fn superblock_offsets() {
    use sb::off::*;
    assert_eq!(
        [
            UUID, LOGSTART, ROOTINO, AGBLOCKS, LOGBLOCKS, VERSIONNUM, FNAME
        ],
        [32, 48, 56, 84, 96, 100, 108]
    );
    assert_eq!(
        [BLOCKLOG, AGBLKLOG, IMAX_PCT, ICOUNT, FDBLOCKS],
        [120, 124, 127, 128, 144]
    );
    assert_eq!(
        [INOALIGNMT, LOGSUNIT, FEATURES2, FEATURES_INCOMPAT],
        [180, 196, 200, 216]
    );
    assert_eq!([CRC, META_UUID], [224, 248]);
}

#[test]
fn ag_header_offsets() {
    assert_eq!(
        [ag::agf::FLFIRST, ag::agf::UUID, ag::agf::LSN, ag::agf::CRC],
        [40, 64, 208, 216]
    );
    assert_eq!(
        [ag::agi::UNLINKED, ag::agi::UUID, ag::agi::CRC, ag::agi::LSN],
        [40, 296, 312, 320]
    );
    assert_eq!([ag::agi::FREE_ROOT, ag::agi::FBLOCKS], [328, 340]);
    assert_eq!([ag::agfl::LSN, ag::agfl::CRC, ag::agfl::BNO], [24, 32, 36]);
    assert_eq!(
        [
            btree::off::BLKNO,
            btree::off::OWNER,
            btree::off::CRC,
            btree::off::RECS
        ],
        [16, 48, 52, 56]
    );
}

#[test]
fn inode_and_log_offsets() {
    use inode::off::*;
    assert_eq!(
        [NLINK, ATIME, SIZE, NEXTENTS, FORKOFF, AFORMAT],
        [16, 32, 56, 76, 82, 83]
    );
    assert_eq!(
        [FLAGS, GEN, NEXT_UNLINKED, CRC, CHANGECOUNT, FLAGS2],
        [90, 92, 96, 100, 104, 120]
    );
    assert_eq!([CRTIME, INO, UUID, DATA], [144, 152, 160, 176]);
    assert_eq!(
        [
            log::off::CRC,
            log::off::CYCLE_DATA,
            log::off::FMT,
            log::off::SIZE
        ],
        [32, 44, 300, 320]
    );
}

#[test]
fn superblock_matches_mkfs() {
    let mkfs = sector(
        512,
        &[
            (0x00, "5846 5342 0000 1000 0000 0000 0000 4000"),
            (0x20, "5153 3430 0000 4000 8000 0000 0000 0005"),
            (0x30, "0000 0000 0000 2005 0000 0000 0000 0060"),
            (0x40, "0000 0000 0000 0061 0000 0000 0000 0062"),
            (0x50, "0000 0001 0000 1000 0000 0004 0000 0000"),
            (0x60, "0000 0a00 b4a5 0200 0200 0008 5153 3430"),
            (0x70, "302d 4946 5300 0000 0c09 0903 0c00 0019"),
            (0x80, "0000 0000 0000 0040 0000 0000 0000 003d"),
            (0x90, "0000 0000 0000 35e4"),
            (0xb0, "0000 0000 0000 0004"),
            (0xc0, "0000 0000 0000 0001 0000 018a 0000 018a"),
            (0xd0, "0000 0000 0000 0009 0000 0009 0000 0000"),
            (0xe0, "57ee 1a06"),
        ],
    );
    let ours = sb::build(
        &geometry(),
        &UUID,
        b"QS400-IFS",
        &sb::Counts {
            icount: 64,
            ifree: 61,
        },
    );
    assert_eq!(ours, mkfs);
}

#[test]
fn agf_matches_mkfs() {
    let mkfs = sector(
        512,
        &[
            (0x00, "5841 4746 0000 0001 0000 0000 0000 1000"),
            (0x10, "0000 0001 0000 0002 0000 0000 0000 0001"),
            (0x20, "0000 0001 0000 0000 0000 0001 0000 0004"),
            (0x30, "0000 0004 0000 0fef 0000 0fec 0000 0000"),
            (0x40, "5153 3430 0000 4000 8000 0000 0000 0005"),
            (0xd0, "0000 0000 0000 0000 e041 71ac"),
        ],
    );
    assert_eq!(ag::build_agf(&geometry(), 0, &UUID), mkfs);
}

#[test]
fn log_record_matches_mkfs() {
    let mkfs = sector(
        1024,
        &[
            (0x000, "feed babe 0000 0001 0000 0002 0000 0200"),
            (0x010, "0000 0001 0000 0000 0000 0001 0000 0000"),
            (0x020, "0000 0000 ffff ffff 0000 0001 b0c0 d0d0"),
            (0x120, "0000 0000 0000 0000 0000 0000 0000 0001"),
            (0x130, "5153 3430 0000 4000 8000 0000 0000 0005"),
            (0x140, "0000 8000"),
            (0x200, "0000 0001 0000 0008 aa20 0000 6e55 0000"),
        ],
    );
    assert_eq!(log::clean_record(512, &UUID), mkfs);
    // 4 KiB sectors: one header and seven cycle-stamped data blocks.
    let big = log::clean_record(4096, &UUID);
    assert_eq!(big.len(), 4096);
    assert_eq!(&big[12..16], &[0, 0, 0x0e, 0]);
    assert_eq!(&big[0xe00..0xe04], &[0, 0, 0, 1]);
}

#[test]
fn root_inode_matches_mkfs() {
    let t = Timestamp {
        secs: 1_792_854_247,
        nsecs: 13_483_000,
    };
    let ours = inode::build(inode::Kind::Root, 96, 96, t, &UUID);
    // mkfs's root inode, its timestamps the same instant.
    let mkfs = sector(
        512,
        &[
            (0x00, "494e 41ed 0301 0000 0000 0000 0000 0000"),
            (0x10, "0000 0002"),
            (0x20, "36ae e470 4f4a 01f8 36ae e470 4f4a 01f8"),
            (0x30, "36ae e470 4f4a 01f8 0000 0000 0000 0006"),
            (0x50, "0000 0002"),
            (0x60, "ffff ffff 421f 9228 0000 0000 0000 0002"),
            (0x70, "0000 0000 0000 0000 0000 0000 0000 0008"),
            (0x90, "36ae e470 4f4a 01f8 0000 0000 0000 0060"),
            (0xa0, "5153 3430 0000 4000 8000 0000 0000 0005"),
            (0xb0, "0000 0000 0060"),
        ],
    );
    assert_eq!(ours, mkfs);
}
