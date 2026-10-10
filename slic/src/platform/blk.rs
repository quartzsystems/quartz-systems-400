// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Block device ioctls: ask the kernel to re-read a partition table.

#![allow(unsafe_code)]

use std::fs::File;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rustix::io::Errno;
use rustix::ioctl::{NoArg, Opcode, ioctl, opcode};

/// `_IO(0x12, 95)`: `BLKRRPART`.
const BLKRRPART: Opcode = opcode::none(0x12, 95);

/// Re-read the partition table of an open whole-disk device.
pub fn reread_partitions(disk: &File) -> Result<(), Errno> {
    // SAFETY: BLKRRPART takes no argument and writes nothing to user
    // memory; it only asks the kernel to rescan the device.
    unsafe { ioctl(disk, NoArg::<BLKRRPART>::new()) }
}

/// Wait up to `timeout` for `/sys/class/block/<name>` to appear.
pub fn wait_for_block(name: &str, timeout: Duration) -> bool {
    let sys = Path::new("/sys/class/block").join(name);
    let dev = Path::new("/dev").join(name);
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if sys.exists() && dev.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}
