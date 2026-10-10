// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 0 mount table.

use std::ffi::CStr;
use std::fs;

use rustix::io::Errno;
use rustix::mount::{MountFlags, mount};

/// One entry of the table.
pub struct Spec {
    /// Device or pseudo-source name.
    pub source: &'static str,
    /// Mount point.
    pub target: &'static str,
    /// Filesystem type.
    pub fstype: &'static str,
    /// Mount flags.
    pub flags: MountFlags,
    /// Filesystem options.
    pub data: Option<&'static CStr>,
}

const NOSUID_NOEXEC_NODEV: MountFlags = MountFlags::NOSUID
    .union(MountFlags::NOEXEC)
    .union(MountFlags::NODEV);

/// Mounted in order.
pub const TABLE: [Spec; 4] = [
    Spec {
        source: "proc",
        target: "/proc",
        fstype: "proc",
        flags: NOSUID_NOEXEC_NODEV,
        data: None,
    },
    Spec {
        source: "sysfs",
        target: "/sys",
        fstype: "sysfs",
        flags: NOSUID_NOEXEC_NODEV,
        data: None,
    },
    Spec {
        source: "devtmpfs",
        target: "/dev",
        fstype: "devtmpfs",
        flags: MountFlags::NOSUID,
        data: Some(c"mode=0755"),
    },
    Spec {
        source: "tmpfs",
        target: "/run",
        fstype: "tmpfs",
        flags: MountFlags::NOSUID.union(MountFlags::NODEV),
        data: Some(c"mode=0755"),
    },
];

/// What happened to one mount point.
pub enum Outcome {
    /// Mounted now.
    Mounted,
    /// Something was already mounted there (`EBUSY`); left alone.
    AlreadyMounted,
}

/// Mount the whole table. Failures are reported, not fatal.
pub fn mount_all() -> Vec<(&'static str, Result<Outcome, Errno>)> {
    TABLE
        .iter()
        .map(|s| {
            let _ = fs::create_dir_all(s.target);
            let result = match mount(s.source, s.target, s.fstype, s.flags, s.data) {
                Ok(()) => Ok(Outcome::Mounted),
                Err(Errno::BUSY) => Ok(Outcome::AlreadyMounted),
                Err(e) => Err(e),
            };
            (s.target, result)
        })
        .collect()
}
