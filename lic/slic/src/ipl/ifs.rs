// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS root's filesystem at IPL. It lives in ASP 1 (the IFS volume,
//! `docs/IFS.md`): `slic` creates and formats the volume when ASP 1 has
//! none, and the storage service serves and mounts it at
//! [`MOUNT_POINT`] (`crate::stgsvc::ifsvol`). A system installed before
//! keeps its `QS400-IFS` partition, which `slic` mounts before the
//! service starts and unmounts once it has closed. A missing or
//! unmountable IFS is an attention code, never the end of the IPL.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use store::Store;
use store::asp::ifs::ops::DEFAULT_THRESHOLD;

use crate::ipl::storage;
use crate::ipl::system::{System, now_unix};
use crate::platform::ifs as plat;
use crate::src;

/// Where the IFS root's filesystem is mounted.
pub const MOUNT_POINT: &str = "/run/qs400/ifs";
/// `/QOpenSys` again, with programs allowed (PASE; `qsh/pase`).
pub const QOPENSYS_EXEC: &str = "/run/qs400/qopensys";

const MIB: u64 = 1024 * 1024;

/// The IFS partition of a system installed before the IFS moved into
/// the ASP, preferring the load source's.
fn partition() -> Option<PathBuf> {
    let lic = licpart::locate().ok();
    diskimg::locate::ifs(lic.as_deref()).ok()
}

/// Whether this system keeps its IFS in a partition: its store names
/// no volume and the partition is there.
fn legacy(system: &System) -> bool {
    system
        .storage
        .as_ref()
        .is_none_or(|s| s.store.ifs_volume().is_empty())
        && partition().is_some()
}

/// Make sure ASP 1 holds a formatted IFS volume (C6001011 when it is
/// made now; A6001271 when the ASP has no room). After the store has
/// opened, before the storage service starts. A system that keeps its
/// partition is left as it is.
pub fn prepare(system: &mut System, installed: bool) {
    if (system.boot.ipl_d && !installed) || legacy(system) {
        return;
    }
    let Some(s) = system.storage.as_mut() else {
        return;
    };
    let store = &mut s.store;
    let made = store.ifs_volume().is_empty();
    if made && let Err(e) = store.ifs_create(DEFAULT_THRESHOLD) {
        system.out.serial(&format!("slic: IFS not created in ASP 1: {e}\n"));
        system.emit(&src::IFS_NOT_CREATED);
        return;
    }
    if store.ifs_formatted() {
        return;
    }
    let pages = store.ifs_volume().pages();
    match format(store) {
        Ok(()) => {
            system.out.serial(&format!(
                "slic: IFS created in ASP 1: {} MiB, xfs\n",
                pages * 4096 / MIB
            ));
            system.emit(&src::IFS_CREATED);
        }
        Err(e) => {
            system.out.serial(&format!("slic: IFS volume not formatted: {e}\n"));
            system.emit(&src::IFS_NOT_CREATED);
        }
    }
}

/// Write xfs onto the volume through the store and mark it formatted.
fn format(store: &mut Store) -> Result<(), String> {
    let bytes = store.ifs_volume().pages() * 4096;
    let ag = u32::try_from(store.ifs_ag_pages()).map_err(|e| e.to_string())?;
    let uuid = uuid::Builder::from_random_bytes(storage::fresh_uuid()).into_uuid();
    let time = diskimg::xfsfmt::Timestamp {
        secs: i64::try_from(now_unix()).unwrap_or(0),
        nsecs: 0,
    };
    let mut opts = diskimg::xfsfmt::Options::new(uuid, time);
    // The device's blocks are 4096 bytes; the sectors may not be less.
    opts.sector_size = 4096;
    opts.ag_blocks = Some(ag);
    {
        let mut f = store.ifs_file();
        diskimg::xfsfmt::format(&mut f, bytes, &opts).map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
    }
    store.ifs_set_formatted().map_err(|e| e.to_string())
}

/// Mount a system's IFS partition (C6001012), or record why not
/// (A6001270, A6001275). `installed`: this IPL has just installed the
/// operating system. The volume in the ASP is the service's to mount.
pub fn mount(system: &mut System, installed: bool) {
    // A D IPL runs from the media and may repartition the disk, which
    // the kernel refuses while a partition on it is mounted; once it has
    // installed the operating system the disk is laid out, and a
    // recovery restores the IFS before the IPL ends.
    if (system.boot.ipl_d && !installed) || plat::mounted(MOUNT_POINT).is_some() {
        return;
    }
    if system
        .storage
        .as_ref()
        .is_some_and(|s| !s.store.ifs_volume().is_empty())
    {
        return;
    }
    let Some(dev) = partition() else {
        // Neither a volume nor a partition: prepare said why.
        return;
    };
    match plat::mount_xfs(&dev, MOUNT_POINT) {
        Ok(()) => {
            system.out.serial(&format!(
                "slic: IFS {} mounted at {MOUNT_POINT}\n",
                dev.display()
            ));
            system.emit(&src::IFS_MOUNTED);
        }
        Err(e) => {
            system
                .out
                .serial(&format!("slic: IFS {}: mount: {e}\n", dev.display()));
            system.emit(&src::IFS_MOUNT_FAILED);
        }
    }
}

/// Mount `/QOpenSys` again with programs allowed, for PASE. After the
/// storage service has started: it makes `/QOpenSys` on a new IFS.
pub fn mount_qopensys(system: &mut System) {
    let qopensys = Path::new(MOUNT_POINT).join("QOpenSys");
    if plat::mounted(MOUNT_POINT).is_none()
        || plat::mounted(QOPENSYS_EXEC).is_some()
        || !qopensys.is_dir()
    {
        return;
    }
    if let Err(e) = plat::mount_exec(&qopensys, QOPENSYS_EXEC) {
        system.out.serial(&format!("slic: {QOPENSYS_EXEC}: mount: {e}\n"));
    }
}

/// Unmount at power down (D1001012): a partition `slic` mounted. The
/// service has unmounted a volume in the ASP before it closed. A
/// filesystem still busy is detached instead, which the kernel finishes
/// once it is let go.
pub fn unmount(system: &mut System) {
    if plat::mounted(MOUNT_POINT).is_none() {
        return;
    }
    rustix::fs::sync();
    if plat::mounted(QOPENSYS_EXEC).is_some() {
        let _ = plat::unmount_xfs(QOPENSYS_EXEC);
    }
    match plat::unmount_xfs(MOUNT_POINT) {
        Ok(()) => system.emit(&src::IFS_UNMOUNTED),
        Err(e) => {
            system.out.serial(&format!("slic: unmount IFS: {e}\n"));
            let _ = rustix::mount::unmount(MOUNT_POINT, rustix::mount::UnmountFlags::DETACH);
        }
    }
}

/// `ifs`: the mount, its capacity and the filesystem's identity.
pub fn report() -> String {
    let mut out = String::new();
    let Some((source, fstype, opts)) = plat::mounted(MOUNT_POINT) else {
        return match partition() {
            Some(dev) => format!("IFS not mounted; partition {}\n", dev.display()),
            None => "IFS not mounted\n".to_string(),
        };
    };
    let _ = writeln!(out, "IFS {source} on {MOUNT_POINT} type {fstype} ({opts})");
    if let Ok(mut f) = std::fs::File::open(&source)
        && let Ok(Some(info)) = diskimg::xfsfmt::probe(&mut f)
    {
        let _ = writeln!(out, "label {} uuid {}", info.label, info.uuid);
    }
    if let Some((size, free, avail)) = plat::capacity(MOUNT_POINT) {
        let _ = writeln!(
            out,
            "capacity {size} bytes ({} MiB), free {free} ({} MiB), available {avail}",
            size / MIB,
            free / MIB
        );
    }
    out
}

/// Lines for the DST LIC display: the IFS in ASP 1 (or its partition)
/// and whether it is mounted.
pub fn lic_info(system: &System) -> Vec<(String, String)> {
    let volume = system
        .storage
        .as_ref()
        .map_or(0, |s| s.store.ifs_volume().pages());
    let what = if volume > 0 {
        format!("In ASP 1, {} MiB, xfs", volume * 4096 / MIB)
    } else if let Some(dev) = partition() {
        let name = dev
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("Partition {name}, xfs")
    } else {
        return vec![("IFS".into(), "Not created".into())];
    };
    let state = match plat::mounted(MOUNT_POINT).and(plat::capacity(MOUNT_POINT)) {
        Some((size, free, _)) => format!(
            "Mounted, {} MiB, {} MiB free",
            size / MIB,
            free / MIB
        ),
        None => "Not mounted".to_string(),
    };
    vec![("IFS".into(), what), ("IFS state".into(), state)]
}

/// RCLSTG's IFS line: where the IFS is. Volume pages and the map are
/// kept by the store's own reclaim.
pub fn reclaim(system: &System) -> String {
    let volume = system
        .storage
        .as_ref()
        .map_or(0, |s| s.store.ifs_volume().pages());
    if volume > 0 {
        return format!("IFS volume in ASP 1: {} MiB, held\n", volume * 4096 / MIB);
    }
    match partition() {
        Some(dev) => format!("IFS partition {}: present\n", dev.display()),
        None => "IFS: none; the next IPL creates it in ASP 1\n".to_string(),
    }
}
