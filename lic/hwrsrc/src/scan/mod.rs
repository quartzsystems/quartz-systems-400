// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading sysfs: PCI functions, Ethernet ports, storage controllers and
//! the small helpers the walk shares. Every path is under a root, `/sys`
//! on the machine and a temporary tree in tests.

pub mod nic;
pub mod pci;
pub mod storage;

use std::fs;
use std::path::Path;

/// Entries of a directory, sorted. Empty if it does not exist.
#[must_use]
pub fn sorted_dir(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// A sysfs attribute, trimmed. Empty if unreadable.
#[must_use]
pub fn read_trim(path: &Path) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Canonical path under a sysfs root, without the root: the device path
/// uevents report.
#[must_use]
pub fn devpath_in(root: &Path, sys: &Path) -> Option<String> {
    let real = fs::canonicalize(sys).ok()?;
    let root = fs::canonicalize(root).ok()?;
    let rest = real.strip_prefix(&root).ok()?;
    Some(format!("/{}", rest.to_string_lossy()))
}

/// A device's serial number as the kernel reports it: the `serial`
/// attribute (virtio, `NVMe`, ATA), else the SCSI `wwid`. Empty if none.
#[must_use]
pub fn serial_of(dev: &Path) -> String {
    ["serial", "wwid", "device/serial", "device/wwid"]
        .iter()
        .map(|a| read_trim(&dev.join(a)))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}
