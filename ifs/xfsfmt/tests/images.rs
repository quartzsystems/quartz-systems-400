// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Image files for the Linux check (tools/conformance/xfs/verify.sh):
//! sparse files of several sizes, with and without reflink, formatted,
//! for `xfs_repair -n`, the kernel's mount and file traffic. Ignored
//! unless run on purpose:
//! `QS400_XFS_DIR=DIR cargo test -p xfsfmt --test images -- --ignored`.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use xfsfmt::{Options, Timestamp, format};

const MIB: u64 = 1024 * 1024;
const TIB: u64 = 1024 * 1024 * MIB;

fn dir(var: &str) -> PathBuf {
    let dir = PathBuf::from(std::env::var(var).expect(var));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One image: `NAME-SECTOR[-reflink].img`.
fn write(dir: &Path, name: &str, bytes: u64, sector_size: u32, ag: Option<u32>, reflink: bool) {
    let kind = if reflink { "-reflink" } else { "" };
    let path = dir.join(format!("{name}-{sector_size}{kind}.img"));
    let _ = std::fs::remove_file(&path);
    let mut f = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    f.set_len(bytes).unwrap();
    let o = Options {
        sector_size,
        zeroed: sector_size == 512 || bytes >= TIB,
        ag_blocks: ag,
        reflink,
        ..Options::new(
            uuid::Uuid::from_u128(0x5153_3430_0000_4000_8000_0000_0000_0005),
            Timestamp {
                secs: 1_792_854_247,
                nsecs: 0,
            },
        )
    };
    let g = format(&mut f, bytes, &o).unwrap();
    println!("{} {:?}", path.display(), g);
}

#[test]
#[ignore = "writes image files for verify.sh"]
fn write_images() {
    let dir = dir("QS400_XFS_DIR");
    let sizes = [
        ("64m", 64 * MIB),
        ("512m", 512 * MIB),
        ("1g", 1024 * MIB),
        ("20g", 20 * 1024 * MIB),
        ("odd", 777 * MIB + 12345),
    ];
    for (name, bytes) in sizes {
        for reflink in [false, true] {
            for sector_size in [512u32, 4096] {
                write(&dir, name, bytes, sector_size, None, reflink);
            }
        }
    }
    // Fixed AG sizes: sixteen AGs, and one (the log in AG 0).
    for sector_size in [512u32, 4096] {
        write(&dir, "16ag", 1029 * MIB, sector_size, Some(16_384), true);
        for reflink in [false, true] {
            write(&dir, "1ag", 100 * MIB, sector_size, Some(16_384), reflink);
        }
    }
}

/// 1 TiB and 64 TiB, with and without reflink: the directory must take
/// sparse files past ext4's 16 TiB (a tmpfs does).
#[test]
#[ignore = "writes image files for verify.sh"]
fn write_big_images() {
    let dir = dir("QS400_XFS_BIG_DIR");
    for (name, bytes) in [("1t", TIB), ("64t", 64 * TIB)] {
        for reflink in [false, true] {
            write(&dir, name, bytes, 4096, None, reflink);
        }
    }
}
