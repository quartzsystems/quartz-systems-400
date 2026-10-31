// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Physical optical media, read only: CD and DVD drives (`sr*`) and
//! removable USB disks, named `OPTnn` from a sorted scan, and the ISO
//! 9660 volume on one, wherever on the device it starts. D-IPL and
//! `RSTLICPGM` from a burned disc or a stick read through here.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::iso::IsoImage;
use crate::iso::hybrid::{self, LBA};
use crate::iso::read::is_iso;
use crate::source::{ReadAt, Slice};

/// What a device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceKind {
    /// A CD or DVD drive.
    Optical,
    /// A removable USB disk (a stick).
    UsbDisk,
}

/// One device from a scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDevice {
    /// Kernel name, `sr0` or `sdb`.
    pub name: String,
    /// What it is.
    pub kind: DeviceKind,
    /// Bytes (0 when no medium is in the drive).
    pub size: u64,
}

fn read_trim(p: &Path) -> String {
    fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Scan a `/sys/class/block` style directory: `sr*` drives, and whole
/// disks that are removable and sit behind USB.
#[must_use]
pub fn scan(sys_block: &Path) -> Vec<BlockDevice> {
    let Ok(dir) = fs::read_dir(sys_block) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let sys = sys_block.join(&name);
        let size = read_trim(&sys.join("size")).parse::<u64>().unwrap_or(0) * 512;
        let kind = if name.starts_with("sr") || read_trim(&sys.join("device/type")) == "5" {
            DeviceKind::Optical
        } else if !sys.join("partition").exists() && read_trim(&sys.join("removable")) == "1" {
            let real = fs::canonicalize(&sys).unwrap_or_default();
            let marker = sys.join("usb");
            if !real.to_string_lossy().contains("/usb") && !marker.exists() {
                continue;
            }
            DeviceKind::UsbDisk
        } else {
            continue;
        };
        out.push(BlockDevice { name, kind, size });
    }
    out
}

/// Kernel names in their natural order: `sr2` before `sr10`, `sdb`
/// before `sdaa`.
fn natural(a: &str) -> (usize, String, u64) {
    let digits = a.trim_start_matches(|c: char| !c.is_ascii_digit());
    let stem = &a[..a.len() - digits.len()];
    (stem.len(), stem.to_string(), digits.parse().unwrap_or(0))
}

/// Name devices as optical resources: drives first, then USB disks,
/// each in natural kernel-name order, `OPT01`, `OPT02` ... The same
/// devices always get the same names.
#[must_use]
pub fn name_resources(devs: &[BlockDevice]) -> Vec<(String, BlockDevice)> {
    let mut v: Vec<&BlockDevice> = devs.iter().collect();
    v.sort_by_key(|d| (d.kind, natural(&d.name)));
    v.dedup_by(|a, b| a.name == b.name);
    v.into_iter()
        .enumerate()
        .map(|(i, d)| (format!("OPT{:02}", i + 1), d.clone()))
        .collect()
}

/// The ISO 9660 volume on a device: at its start (a disc, a hybrid image
/// written raw), else at the start of a GPT or MBR partition (an ISO
/// written into a partition).
pub fn open_media<R: ReadAt + Clone>(dev: R) -> Result<IsoImage<Slice<R>>> {
    let size = dev.size()?;
    if is_iso(&dev) {
        return IsoImage::open(Slice::new(dev, 0, size));
    }
    for (start, len) in partitions(&dev) {
        let part = Slice::new(dev.clone(), start, len);
        if is_iso(&part) {
            return IsoImage::open(part);
        }
    }
    Err(Error::bad("no ISO 9660 volume on the device"))
}

/// Partitions as `(byte offset, bytes)`: the GPT's when it checks out,
/// else the MBR's primary entries.
fn partitions(dev: &dyn ReadAt) -> Vec<(u64, u64)> {
    if let Ok(gpt) = hybrid::verify(dev) {
        return gpt
            .partitions
            .iter()
            .filter(|p| p.last_lba >= p.first_lba)
            .map(|p| (p.first_lba * LBA, (p.last_lba - p.first_lba + 1) * LBA))
            .collect();
    }
    let mut mbr = [0u8; 512];
    if dev.read_exact_at(0, &mut mbr).is_err() || mbr[510] != 0x55 || mbr[511] != 0xAA {
        return Vec::new();
    }
    let mut v = Vec::new();
    for i in 0..4 {
        let e = &mbr[446 + 16 * i..462 + 16 * i];
        let start = u64::from(u32::from_le_bytes(e[8..12].try_into().expect("4")));
        let n = u64::from(u32::from_le_bytes(e[12..16].try_into().expect("4")));
        if e[4] != 0 && e[4] != 0xEE && n > 0 {
            v.push((start * LBA, n * LBA));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(name: &str, kind: DeviceKind) -> BlockDevice {
        BlockDevice {
            name: name.into(),
            kind,
            size: 0,
        }
    }

    #[test]
    fn names_are_stable() {
        let devs = vec![
            dev("sdc", DeviceKind::UsbDisk),
            dev("sr10", DeviceKind::Optical),
            dev("sdaa", DeviceKind::UsbDisk),
            dev("sr2", DeviceKind::Optical),
            dev("sdb", DeviceKind::UsbDisk),
        ];
        let named: Vec<(String, String)> = name_resources(&devs)
            .into_iter()
            .map(|(n, d)| (n, d.name))
            .collect();
        let want = [
            ("OPT01", "sr2"),
            ("OPT02", "sr10"),
            ("OPT03", "sdb"),
            ("OPT04", "sdc"),
            ("OPT05", "sdaa"),
        ];
        assert_eq!(
            named,
            want.iter()
                .map(|(a, b)| ((*a).into(), (*b).into()))
                .collect::<Vec<(String, String)>>()
        );
        let mut rev = devs.clone();
        rev.reverse();
        assert_eq!(name_resources(&rev), name_resources(&devs));
    }
}
