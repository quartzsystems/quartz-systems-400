// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sync, unmount, reboot.

use rustix::io::Errno;
use rustix::mount::{UnmountFlags, unmount};
use rustix::system::{RebootCommand, reboot};

use crate::platform::mounts::TABLE;

/// Flush every filesystem.
pub fn sync() {
    rustix::fs::sync();
}

/// Unmount the Phase 1 table in reverse order. Returns what failed.
/// Lazy detach: slic still holds `/dev/console` and the serial port open to
/// report the result, and nothing on these pseudo filesystems needs a
/// synchronous unmount.
pub fn unmount_all() -> Vec<(&'static str, Errno)> {
    let mut failed = Vec::new();
    for spec in TABLE.iter().rev() {
        if let Err(e) = unmount(spec.target, UnmountFlags::DETACH) {
            failed.push((spec.target, e));
        }
    }
    failed
}

/// Power off, or restart when `restart` is set. Only returns on failure.
pub fn reboot_now(restart: bool) -> Errno {
    let cmd = if restart {
        RebootCommand::Restart
    } else {
        RebootCommand::PowerOff
    };
    match reboot(cmd) {
        Ok(()) => Errno::IO,
        Err(e) => e,
    }
}
