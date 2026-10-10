// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Finding the partition by GPT type GUID through sysfs. Device names are
//! never assumed.

use uuid::{Uuid, uuid};

/// GPT partition type GUID of the QS/400 LIC partition. Generated once for
/// Quartz Systems; never reuse it for another partition kind.
pub const LIC_TYPE_GUID: Uuid = uuid!("a0b7c2f4-5e13-4d9a-9f31-515334304c49");
/// GPT partition name.
pub const PARTITION_NAME: &str = "QS400-LIC";

#[cfg(target_os = "linux")]
pub use linux::locate;

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io::{self, Read, Seek, SeekFrom};
    use std::path::{Path, PathBuf};

    use uuid::Uuid;

    use super::LIC_TYPE_GUID;
    use crate::error::LicError;

    const SYS_BLOCK: &str = "/sys/class/block";

    /// `/dev/<partition>` of the first LIC partition found, scanning whole
    /// disks under `/sys/class/block` in name order.
    pub fn locate() -> Result<PathBuf, LicError> {
        let mut names: Vec<String> = fs::read_dir(SYS_BLOCK)?
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let sys = Path::new(SYS_BLOCK).join(&name);
            if sys.join("partition").exists() {
                continue; // a partition, not a disk
            }
            let Ok(Some(index)) = gpt_find(&Path::new("/dev").join(&name)) else {
                continue;
            };
            if let Some(part) = child_partition(&sys, index) {
                return Ok(Path::new("/dev").join(part));
            }
        }
        Err(LicError::NotFound)
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

    /// 1-based index of the partition with the LIC type GUID, if any.
    fn gpt_find(dev: &Path) -> io::Result<Option<u32>> {
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
            for i in 0..count {
                f.read_exact(&mut entry)?;
                let guid = Uuid::from_bytes_le(entry[..16].try_into().expect("16"));
                if guid == LIC_TYPE_GUID {
                    return Ok(Some(i + 1));
                }
            }
            return Ok(None);
        }
        Ok(None)
    }
}
