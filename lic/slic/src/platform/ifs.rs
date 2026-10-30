// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS filesystem's mount: xfs on its partition, the kernel's view
//! of it, and its capacity.

use std::fs;
use std::path::Path;

use rustix::io::Errno;
use rustix::mount::{MountFlags, UnmountFlags, mount, unmount};

/// Mount the xfs on `dev` at `target`: no devices, set-ID or programs.
pub fn mount_xfs(dev: &Path, target: &str) -> Result<(), Errno> {
    let _ = fs::create_dir_all(target);
    let flags = MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC;
    mount(dev, target, "xfs", flags, None)
}

/// Mount `dir` (a directory of the IFS mount) again at `target` with
/// programs allowed, still without devices or set-ID: PASE's `/QOpenSys`.
pub fn mount_exec(dir: &Path, target: &str) -> Result<(), Errno> {
    use rustix::mount::{MountPropagationFlags, mount_bind, mount_change, mount_remount};
    let _ = fs::create_dir_all(target);
    mount_bind(dir, target)?;
    let _ = mount_change(target, MountPropagationFlags::PRIVATE);
    mount_remount(target, MountFlags::BIND | MountFlags::NOSUID | MountFlags::NODEV, "")
}

/// Unmount `target`, waiting for the filesystem to be written out.
pub fn unmount_xfs(target: &str) -> Result<(), Errno> {
    unmount(target, UnmountFlags::empty())
}

/// What is mounted at `target`: source, type and options.
pub fn mounted(target: &str) -> Option<(String, String, String)> {
    let table = fs::read_to_string("/proc/self/mounts").ok()?;
    table.lines().rev().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        (f.len() >= 4 && f[1] == target).then(|| (f[0].into(), f[2].into(), f[3].into()))
    })
}

/// Bytes in all, free, and free to unprivileged writers.
pub fn capacity(target: &str) -> Option<(u64, u64, u64)> {
    let s = rustix::fs::statvfs(target).ok()?;
    Some((
        s.f_blocks * s.f_frsize,
        s.f_bfree * s.f_frsize,
        s.f_bavail * s.f_frsize,
    ))
}
