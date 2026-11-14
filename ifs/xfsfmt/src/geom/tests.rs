// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Geometries against what mkfs.xfs 6.18 lays out.

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
fn reflink_matches_mkfs() {
    // mkfs.xfs 6.18 -m rmapbt=1,reflink=1 -d agcount=4 -l size=3584b.
    let g = Geometry::for_features(64 * MIB, 512, None, true).unwrap();
    assert_eq!(g.log_blocks, 3584);
    assert_eq!(g.log_start(), 8199);
    assert_eq!(g.reflink_roots(), Some([5, 6]));
    assert_eq!(g.agfl_blocks(0), 7..13);
    assert_eq!(g.agfl_blocks(2), 3591..3597);
    assert_eq!(g.root_ino(), 128);
    assert_eq!(g.free_extents(0), vec![(13, 3), (24, 4072)]);
    assert_eq!(g.free_blocks(1), 4083);
    assert_eq!(g.free_blocks(2), 499);
    assert_eq!(g.fdblocks(), 12764);
    let (fs, ag, ino, inodes, refc, log) = (
        RmapOwner::Fs,
        RmapOwner::Ag,
        RmapOwner::Inobt,
        RmapOwner::Inodes,
        RmapOwner::Refc,
        RmapOwner::Log,
    );
    assert_eq!(
        g.rmap_records(0),
        vec![
            (0, 1, fs),
            (1, 2, ag),
            (3, 2, ino),
            (5, 1, ag),
            (6, 1, refc),
            (7, 6, ag),
            (16, 8, inodes)
        ]
    );
    assert_eq!(g.rmap_records(2)[5..], [(7, 3584, log), (3591, 6, ag)]);
    assert_eq!(RmapOwner::Refc.value(), (-8i64).cast_unsigned());
}

#[test]
fn reflink_logs_pass_the_kernel_minimum() {
    // The kernel's minimum with rmapbt and reflink, from mkfs.xfs 6.18:
    // 3273 blocks below 300 MiB (AGs up to 76800 blocks), 4287 for
    // 1 TiB AGs.
    for (bytes, ag) in [
        (64 * MIB, None),
        (299 * MIB, None),
        (299 * MIB, Some(76_000)),
        (300 * MIB, None),
        (64 << 40, None),
    ] {
        for sect in [512, 4096] {
            let g = Geometry::for_features(bytes, sect, ag, true).unwrap();
            let min = if g.dblocks * u64::from(BLOCK_SIZE) < 300 * MIB {
                3273
            } else {
                4287
            };
            assert!(g.log_blocks > min, "{bytes} {sect}: {}", g.log_blocks);
        }
    }
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
fn a_fixed_ag_size_keeps_whole_ags_and_leaves_the_rest() {
    let g = Geometry::for_bytes_with(GIB + 5 * MIB, 4096, Some(16_384)).unwrap();
    assert_eq!((g.ag_count, g.ag_blocks), (16, 16_384));
    assert_eq!(g.dblocks, 16 * 16_384);
    assert_eq!(g.log_ag, 8);
    assert!(Geometry::for_bytes_with(64 * MIB, 4096, Some(32_768)).is_err());
}

#[test]
fn odd_sizes_keep_whole_ags() {
    for bytes in [MIN_BYTES + 4095, 100 * MIB + 12345, 513 * MIB + 7] {
        for reflink in [false, true] {
            let g = Geometry::for_features(bytes, 512, None, reflink).unwrap();
            assert!(g.dblocks * u64::from(BLOCK_SIZE) <= bytes);
            assert!(g.agfl_blocks(g.log_ag).end < g.ag_blocks);
            assert_eq!(g.chunk_agbno % INO_ALIGN, 0);
        }
    }
}

#[test]
fn reflink_four_k_sectors_match_mkfs() {
    let g = Geometry::for_features(64 * MIB, 4096, None, true).unwrap();
    assert_eq!(g.log_start(), 8202);
    assert_eq!(g.reflink_roots(), Some([8, 9]));
    assert_eq!(g.agfl_blocks(0), 10..16);
    assert_eq!(g.root_ino(), 128);
    assert_eq!(g.rmap_records(1)[0], (0, 4, RmapOwner::Fs));
}
