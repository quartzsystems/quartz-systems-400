// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding partitions by GPT type GUID, and listing whole disks,
//! through sysfs. Device names are never assumed.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::Error;
use crate::plan::{ASP1_TYPE_GUID, IFS_TYPE_GUID, LIC_TYPE_GUID};

const SYS_BLOCK: &str = "/sys/class/block";

/// A whole disk as the installer sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// Kernel name, `sda` or `vda`.
    pub name: String,
    /// Device node.
    pub dev: PathBuf,
    /// Size in bytes.
    pub bytes: u64,
    /// Model string from sysfs, or empty.
    pub model: String,
    /// Whether a QS/400 LIC, IFS or ASP partition is already on it.
    pub has_qs400: bool,
}

/// The whole disk a partition node belongs to, `nvme0n1` for
/// `/dev/nvme0n1p2`, from its sysfs path. `None` for a whole disk.
#[must_use]
pub fn disk_of(part: &Path) -> Option<String> {
    let name = part.file_name()?.to_str()?;
    let real = fs::canonicalize(Path::new(SYS_BLOCK).join(name)).ok()?;
    let parent = real.parent()?.file_name()?.to_str()?;
    (parent != "block").then(|| parent.to_string())
}

/// What the disk behind a node calls itself: a type/model string and a
/// serial number, from sysfs. Either is empty when sysfs has nothing.
#[must_use]
pub fn identity(node: &Path) -> (String, String) {
    let name = node
        .file_name()
        .and_then(|n| n.to_str())
        .map(ToString::to_string)
        .unwrap_or_default();
    let disk = disk_of(node).unwrap_or(name);
    let sys = Path::new(SYS_BLOCK).join(&disk);
    let read = |rel: &str| {
        fs::read_to_string(sys.join(rel))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let vendor = read("device/vendor");
    let model = read("device/model");
    let model = if model.is_empty() {
        if disk.starts_with("vd") {
            "virtio".to_string()
        } else {
            String::new()
        }
    } else if vendor.is_empty() || model.starts_with(&vendor) {
        model
    } else {
        format!("{vendor} {model}")
    };
    let serial = ["device/serial", "serial", "device/wwid"]
        .iter()
        .map(|rel| read(rel))
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    (model, serial)
}

/// Whole disks under `/sys/class/block` in name order, without
/// removable media and loop or RAM devices.
pub fn whole_disks() -> Result<Vec<DiskInfo>, Error> {
    let mut out = Vec::new();
    for name in disk_names()? {
        let sys = Path::new(SYS_BLOCK).join(&name);
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("sr")
            || name.starts_with("zram")
        {
            continue;
        }
        let sectors: u64 = fs::read_to_string(sys.join("size"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        if sectors == 0 {
            continue;
        }
        let model = fs::read_to_string(sys.join("device/model"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let dev = Path::new("/dev").join(&name);
        let has_qs400 = types_on(&dev).is_ok_and(|t| {
            t.iter()
                .any(|g| *g == LIC_TYPE_GUID || *g == ASP1_TYPE_GUID || *g == IFS_TYPE_GUID)
        });
        out.push(DiskInfo {
            name,
            dev,
            bytes: sectors * 512,
            model,
            has_qs400,
        });
    }
    Ok(out)
}

/// `/dev/<partition>` of every partition with type `guid`, scanning
/// whole disks in name order.
pub fn partitions_of_type(guid: Uuid) -> Result<Vec<PathBuf>, Error> {
    let mut out = Vec::new();
    for name in disk_names()? {
        let sys = Path::new(SYS_BLOCK).join(&name);
        let Ok(types) = types_on(&Path::new("/dev").join(&name)) else {
            continue;
        };
        for (i, t) in types.iter().enumerate() {
            if *t == guid
                && let Some(part) = child_partition(&sys, u32::try_from(i + 1).expect("index"))
            {
                out.push(Path::new("/dev").join(part));
            }
        }
    }
    Ok(out)
}

/// The first partition with type `guid`.
pub fn first_of_type(guid: Uuid) -> Result<PathBuf, Error> {
    partitions_of_type(guid)?
        .into_iter()
        .next()
        .ok_or(Error::NotFound(guid))
}

/// The IFS partition: the one on the same disk as `beside` (the LIC
/// partition) when there is one, else the first in disk name order.
pub fn ifs(beside: Option<&Path>) -> Result<PathBuf, Error> {
    let all = partitions_of_type(IFS_TYPE_GUID)?;
    let home = beside.and_then(disk_of);
    all.iter()
        .find(|p| home.is_some() && disk_of(p) == home)
        .or(all.first())
        .cloned()
        .ok_or(Error::NotFound(IFS_TYPE_GUID))
}

/// Whole disk names (block devices without a `partition` attribute).
fn disk_names() -> Result<Vec<String>, Error> {
    let mut names: Vec<String> = fs::read_dir(SYS_BLOCK)
        .map_err(Error::io("read /sys/class/block"))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !Path::new(SYS_BLOCK).join(n).join("partition").exists())
        .collect();
    names.sort();
    Ok(names)
}

/// Name of the child block device with `partition` == `index`.
fn child_partition(disk_sys: &Path, index: u32) -> Option<String> {
    for entry in fs::read_dir(disk_sys).ok()?.filter_map(Result::ok) {
        let p = entry.path().join("partition");
        if let Ok(text) = fs::read_to_string(&p)
            && text.trim().parse::<u32>().ok() == Some(index)
        {
            return Some(entry.file_name().to_string_lossy().into_owned());
        }
    }
    None
}

/// Partition type GUIDs of a disk in table order (unused entries are
/// nil). Reads the primary GPT at a 512- or 4096-byte sector size.
fn types_on(dev: &Path) -> io::Result<Vec<Uuid>> {
    let mut f = fs::File::open(dev)?;
    for sector in [512u64, 4096] {
        let mut hdr = [0u8; 92];
        f.seek(SeekFrom::Start(sector))?;
        if f.read_exact(&mut hdr).is_err() || &hdr[..8] != b"EFI PART" {
            continue;
        }
        let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().expect("8"));
        let count = u32::from_le_bytes(hdr[80..84].try_into().expect("4"));
        let size = u32::from_le_bytes(hdr[84..88].try_into().expect("4"));
        if size < 128 || count > 1024 {
            continue;
        }
        f.seek(SeekFrom::Start(entries_lba * sector))?;
        let mut entry = vec![0u8; size as usize];
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            f.read_exact(&mut entry)?;
            out.push(Uuid::from_bytes_le(entry[..16].try_into().expect("16")));
        }
        return Ok(out);
    }
    Ok(Vec::new())
}
