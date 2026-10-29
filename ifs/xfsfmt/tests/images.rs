// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Image files for the Linux check (tools/conformance/xfs/verify.sh):
//! sparse files of several sizes, formatted, for `xfs_repair -n`, the
//! kernel's mount and file traffic. Ignored unless run on purpose:
//! `QS400_XFS_DIR=DIR cargo test -p xfsfmt --test images -- --ignored`.

use std::fs::OpenOptions;
use std::path::PathBuf;

use xfsfmt::{Options, Timestamp, format};

const MIB: u64 = 1024 * 1024;

#[test]
#[ignore = "writes image files for verify.sh"]
fn write_images() {
    let dir = PathBuf::from(std::env::var("QS400_XFS_DIR").expect("QS400_XFS_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    let sizes = [
        ("64m", 64 * MIB),
        ("512m", 512 * MIB),
        ("1g", 1024 * MIB),
        ("20g", 20 * 1024 * MIB),
        ("odd", 777 * MIB + 12345),
    ];
    for (name, bytes) in sizes {
        for sector_size in [512u32, 4096] {
            let path = dir.join(format!("{name}-{sector_size}.img"));
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
                zeroed: sector_size == 512,
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
    }
}
