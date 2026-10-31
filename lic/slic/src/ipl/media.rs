// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms o the MIT license which can be found in the
// root directory of this project.

//! The installation media locator. Whatever carries the media (an ISO
//! 9660 CD, the hybrid ISO written raw to a disk or stick, or its tree
//! copied onto FAT32), the mark is `QS400/MEDIA.MANIFEST`. Block
//! devices are tried in order: optical drives, removable disks, then
//! the rest, each disk before its partitions; a device is mounted
//! read-only only when its first sectors look like ISO 9660 or FAT, so
//! the kernel logs nothing about the others.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use optical::manifest::{self, Manifest};
use rustix::mount::{MountFlags, UnmountFlags, mount, unmount};

/// Where a candidate is mounted while it is looked at.
const MOUNT_AT: &str = "/run/qs400/media";
const SYS_BLOCK: &str = "/sys/class/block";

/// Media that was found.
#[derive(Debug, Clone)]
pub struct Found {
    /// The device node, `/dev/sr0` or `/dev/vda1`.
    pub device: String,
    /// The whole disk it is on, `sr0` or `vda`.
    pub disk: String,
    /// `iso9660` or `vfat`.
    pub fstype: &'static str,
    /// The manifest.
    pub manifest: Manifest,
}

static FOUND: Mutex<Option<Found>> = Mutex::new(None);

fn read_trim(p: &Path) -> String {
    fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Block devices to try: `(node name, whole disk)`, in order.
fn candidates() -> Vec<(String, String)> {
    let Ok(dir) = fs::read_dir(SYS_BLOCK) else {
        return Vec::new();
    };
    let mut v: Vec<(u8, String, String)> = Vec::new();
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if ["loop", "ram", "zram", "dm-", "md"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let sys = Path::new(SYS_BLOCK).join(&name);
        let disk = if sys.join("partition").exists() {
            diskimg::locate::disk_of(&Path::new("/dev").join(&name)).unwrap_or_else(|| name.clone())
        } else {
            name.clone()
        };
        let rank = if disk.starts_with("sr") {
            0
        } else if read_trim(&Path::new(SYS_BLOCK).join(&disk).join("removable")) == "1" {
            1
        } else {
            2
        };
        v.push((rank, disk, name));
    }
    // Shorter names first so `vdb` comes before `vdaa`; the disk itself before its parts.
    v.sort_by(|a, b| {
        (a.0, a.1.len(), &a.1, a.2.len(), &a.2).cmp(&(b.0, b.1.len(), &b.1, b.2.len(), &b.2))
    });
    v.into_iter().map(|(_, d, n)| (n, d)).collect()
}

/// What the device's first sectors look like.
fn sniff(dev: &str) -> Option<&'static str> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(dev).ok()?;
    let mut boot = [0u8; 512];
    f.read_exact(&mut boot).ok()?;
    let fat = boot[510] == 0x55
        && boot[511] == 0xAA
        && (boot.get(54..57) == Some(b"FAT") || boot.get(82..87) == Some(b"FAT32"));
    if fat {
        return Some("vfat");
    }
    let mut pvd = [0u8; 6];
    f.seek(SeekFrom::Start(16 * 2048)).ok()?;
    f.read_exact(&mut pvd).ok()?;
    (&pvd[1..6] == b"CD001").then_some("iso9660")
}

/// Mount `dev` read-only, run `f` over the mount point, unmount.
fn with_mount<T>(dev: &str, fstype: &str, f: impl FnOnce(&Path) -> T) -> Result<T, String> {
    fs::create_dir_all(MOUNT_AT).map_err(|e| format!("{MOUNT_AT}: {e}"))?;
    let flags = MountFlags::RDONLY | MountFlags::NOSUID | MountFlags::NODEV | MountFlags::NOEXEC;
    mount(dev, MOUNT_AT, fstype, flags, None::<&std::ffi::CStr>)
        .map_err(|e| format!("mount {dev} ({fstype}): {e}"))?;
    let out = f(Path::new(MOUNT_AT));
    let _ = unmount(MOUNT_AT, UnmountFlags::empty());
    Ok(out)
}

fn try_device(dev: &str) -> Option<(&'static str, Manifest)> {
    let fstype = sniff(dev)?;
    let text = with_mount(dev, fstype, |at| fs::read_to_string(at.join(manifest::PATH)).ok())
        .ok()
        .flatten()?;
    Manifest::parse(&text).ok().map(|m| (fstype, m))
}

/// The installation media: the first block device whose file system
/// holds the manifest. Remembered once found.
pub fn locate() -> Result<Found, String> {
    let mut g = FOUND.lock().map_err(|_| "media lock poisoned".to_string())?;
    if let Some(f) = g.as_ref() {
        return Ok(f.clone());
    }
    let mut tried = Vec::new();
    for (name, disk) in candidates() {
        let dev = format!("/dev/{name}");
        match try_device(&dev) {
            Some((fstype, manifest)) => {
                let f = Found {
                    device: dev,
                    disk,
                    fstype,
                    manifest,
                };
                *g = Some(f.clone());
                return Ok(f);
            }
            None => tried.push(name),
        }
    }
    Err(format!(
        "no QS/400 installation media found (looked at {})",
        if tried.is_empty() {
            "no block device".to_string()
        } else {
            tried.join(", ")
        }
    ))
}

/// The two slot UKIs from the media, checked against the manifest.
pub fn read_payload(f: &Found) -> Result<(Vec<u8>, Vec<u8>), String> {
    let read = |at: &Path, path: &str| -> Result<Vec<u8>, String> {
        let data = fs::read(at.join(path)).map_err(|e| format!("{path} on the media: {e}"))?;
        match f.manifest.file(path) {
            Some(m) if m.size == data.len() as u64 => Ok(data),
            Some(m) => Err(format!(
                "{path} on the media is {} bytes, the manifest says {}",
                data.len(),
                m.size
            )),
            None => Err(format!("{path} is not in the media manifest")),
        }
    };
    with_mount(&f.device, f.fstype, |at| {
        Ok((read(at, manifest::SLOT_A)?, read(at, manifest::SLOT_B)?))
    })?
}
