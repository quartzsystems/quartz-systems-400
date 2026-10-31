// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS root's filesystem: the `QS400-IFS` partition beside the LIC
//! partition, mounted xfs at [`MOUNT_POINT`] before the storage service
//! starts (which inherits the mount) and unmounted at power down once
//! the service has closed. A missing or unmountable IFS is an attention
//! code, never the end of the IPL. RCLSTG adds the partition to a disk
//! laid out before it existed, when the disk has room at its end.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

use diskimg::migrate::AddIfs;

use crate::ipl::storage;
use crate::ipl::system::{System, now_unix};
use crate::platform::ifs as plat;
use crate::src;

/// Where the IFS root's filesystem is mounted.
pub const MOUNT_POINT: &str = "/run/qs400/ifs";
/// `/QOpenSys` again, with programs allowed (PASE; `qsh/pase`).
pub const QOPENSYS_EXEC: &str = "/run/qs400/qopensys";

const MIB: u64 = 1024 * 1024;

/// The IFS partition, preferring the load source's.
fn partition() -> Option<PathBuf> {
    let lic = licpart::locate().ok();
    diskimg::locate::ifs(lic.as_deref()).ok()
}

/// Mount the IFS (C6001012), or record why not (A6001270, A6001275).
/// `installed`: this IPL has just installed the operating system.
pub fn mount(system: &mut System, installed: bool) {
    // A D IPL runs from the media and may repartition the disk, which
    // the kernel refuses while a partition on it is mounted; once it has
    // installed the operating system the disk is laid out, and a
    // recovery restores the IFS before the IPL ends.
    if (system.boot.ipl_d && !installed) || plat::mounted(MOUNT_POINT).is_some() {
        return;
    }
    let Some(dev) = partition() else {
        system.emit(&src::IFS_MISSING);
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

/// Unmount at power down (D1001012). A filesystem still busy is
/// detached instead, which the kernel finishes once it is let go.
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
            None => "IFS not mounted; no IFS partition\n".to_string(),
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

/// Lines for the DST LIC display.
pub fn lic_info() -> Vec<(String, String)> {
    let Some(dev) = partition() else {
        return vec![("IFS".into(), "No IFS partition".into())];
    };
    let name = dev
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (what, state) = match plat::mounted(MOUNT_POINT).and(plat::capacity(MOUNT_POINT)) {
        Some((size, free, _)) => (
            format!("{name}, {} MiB, xfs", size / MIB),
            format!("Mounted, {} MiB free", free / MIB),
        ),
        None => (format!("{name}, xfs"), "Not mounted".to_string()),
    };
    vec![("IFS".into(), what), ("IFS state".into(), state)]
}

/// RCLSTG's IFS line: the partition is there, or (unless `check`) it is
/// added at the end of the load source when the disk has room.
pub fn reclaim(check: bool) -> String {
    if let Some(dev) = partition() {
        return format!("IFS partition {}: present\n", dev.display());
    }
    if check {
        return "IFS partition: missing; RCLSTG *ALL adds it when the load source has room\n"
            .to_string();
    }
    match add_partition() {
        Ok(AddIfs::Added { part, index }) => format!(
            "IFS partition {index} added at the end of the load source ({} MiB); \
             it is mounted at the next IPL\n",
            part.bytes() / MIB
        ),
        Ok(AddIfs::Present) => "IFS partition: present\n".to_string(),
        // BLOCKED(phase10): making room by shrinking ASP 1 needs the
        // store to give back the tail of a unit and the kernel to resize
        // a partition in use; neither exists yet.
        Ok(AddIfs::NoSpace { free }) => format!(
            "IFS partition: missing. The load source has {} MiB unused at its end and the \
             IFS needs {} MiB; taking space from ASP 1 is not available in this release\n",
            free / MIB,
            diskimg::MIN_IFS_BYTES / MIB
        ),
        Err(e) => format!("IFS partition: not added: {e}\n"),
    }
}

/// Add the IFS partition to the disk holding the LIC partition.
fn add_partition() -> Result<AddIfs, String> {
    let lic = licpart::locate().map_err(|e| e.to_string())?;
    let disk = diskimg::locate::disk_of(&lic).ok_or("no load source disk")?;
    let path = format!("/dev/{disk}");
    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("open {path}: {e}"))?;
    let bytes = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    let v4 = |b| uuid::Builder::from_random_bytes(b).into_uuid();
    let ifs = diskimg::IfsFormat {
        time: diskimg::xfsfmt::Timestamp {
            secs: i64::try_from(now_unix()).unwrap_or(0),
            nsecs: 0,
        },
        zeroed: false,
    };
    let r = diskimg::migrate::add_ifs(
        &mut f,
        bytes,
        v4(storage::fresh_uuid()),
        v4(storage::fresh_uuid()),
        &ifs,
    )
    .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    Ok(r)
}
