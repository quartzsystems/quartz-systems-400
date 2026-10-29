// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where the partitions go. The ESP and the LIC partition have fixed
//! sizes; the IFS takes a quarter of what is left (within bounds) and
//! ASP1 the rest. On disk: ESP | LIC | IFS | ASP1. In the GPT the IFS
//! is entry 4, so ASP1 keeps entry 3 as on disks laid out before it.

use uuid::{Uuid, uuid};

use crate::Error;

/// Logical sector size the layout is expressed in.
pub const SECTOR: u64 = 512;
/// First usable LBA: 1 MiB in, after the GPT.
pub const ESP_FIRST_LBA: u64 = 2048;
/// ESP size.
pub const ESP_BYTES: u64 = 256 * 1024 * 1024;
/// LIC partition size.
pub const LIC_BYTES: u64 = 64 * 1024 * 1024;
/// Smallest IFS partition.
pub const MIN_IFS_BYTES: u64 = 512 * 1024 * 1024;
/// Largest IFS partition.
pub const MAX_IFS_BYTES: u64 = 256 * 1024 * 1024 * 1024;
/// Smallest ASP1.
pub const MIN_ASP1_BYTES: u64 = 64 * 1024 * 1024;
/// Sectors kept after the last partition for the backup GPT.
pub const TAIL_SECTORS: u64 = 2048;
/// Default image size: 4 GiB.
pub const DEFAULT_DISK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Smallest disk the layout fits.
pub const MIN_DISK_BYTES: u64 = (ESP_FIRST_LBA + TAIL_SECTORS) * SECTOR
    + ESP_BYTES
    + LIC_BYTES
    + MIN_IFS_BYTES
    + MIN_ASP1_BYTES;

/// GPT partition type of the LIC partition. Generated once for Quartz
/// Systems; never reuse it for another partition kind.
pub const LIC_TYPE_GUID: Uuid = uuid!("a0b7c2f4-5e13-4d9a-9f31-515334304c49");
/// GPT partition type of an ASP unit. Same rule.
pub const ASP1_TYPE_GUID: Uuid = uuid!("3c5a9d7e-2b41-4c6f-9e08-515334304131");
/// GPT partition type of the IFS (an XFS filesystem). Same rule.
pub const IFS_TYPE_GUID: Uuid = uuid!("90afa14f-9e50-4da1-99ae-515334304946");
/// ESP partition name.
pub const ESP_NAME: &str = "QS400 ESP";
/// LIC partition name.
pub const LIC_NAME: &str = "QS400-LIC";
/// IFS partition name (also its filesystem label).
pub const IFS_NAME: &str = "QS400-IFS";
/// ASP1 partition name.
pub const ASP1_NAME: &str = "QS400-ASP1";
/// GPT entry of the IFS partition.
pub const IFS_INDEX: u32 = 4;

/// One partition's sectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// First sector.
    pub first_lba: u64,
    /// Last sector, inclusive.
    pub last_lba: u64,
}

impl Part {
    /// Size in bytes.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        (self.last_lba - self.first_lba + 1) * SECTOR
    }

    /// Byte offset of the first sector.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.first_lba * SECTOR
    }
}

/// The whole layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Disk size in sectors.
    pub total_sectors: u64,
    /// The ESP.
    pub esp: Part,
    /// The LIC partition.
    pub lic: Part,
    /// The IFS partition.
    pub ifs: Part,
    /// The ASP1 partition.
    pub asp1: Part,
}

/// IFS size for `space` bytes after the LIC partition: a quarter,
/// clamped to [`MIN_IFS_BYTES`]..=[`MAX_IFS_BYTES`], in whole 4 KiB.
#[must_use]
pub fn ifs_bytes(space: u64) -> u64 {
    (space / 4).clamp(MIN_IFS_BYTES, MAX_IFS_BYTES) / 4096 * 4096
}

impl Plan {
    /// Lay out a disk of `bytes`. The IFS and ASP1 end on 4 KiB boundaries.
    pub fn for_bytes(bytes: u64) -> Result<Self, Error> {
        if bytes < MIN_DISK_BYTES {
            return Err(Error::TooSmall {
                bytes,
                min: MIN_DISK_BYTES,
            });
        }
        let total_sectors = bytes / SECTOR;
        let esp = Part {
            first_lba: ESP_FIRST_LBA,
            last_lba: ESP_FIRST_LBA + ESP_BYTES / SECTOR - 1,
        };
        let lic = Part {
            first_lba: esp.last_lba + 1,
            last_lba: esp.last_lba + LIC_BYTES / SECTOR,
        };
        let asp_end = (total_sectors - TAIL_SECTORS) / 8 * 8;
        let space = (asp_end - lic.last_lba - 1) * SECTOR;
        let ifs = Part {
            first_lba: lic.last_lba + 1,
            last_lba: lic.last_lba + ifs_bytes(space) / SECTOR,
        };
        Ok(Self {
            total_sectors,
            esp,
            lic,
            ifs,
            asp1: Part {
                first_lba: ifs.last_lba + 1,
                last_lba: asp_end - 1,
            },
        })
    }

    /// The default 4 GiB image.
    #[must_use]
    pub fn default_image() -> Self {
        Self::for_bytes(DEFAULT_DISK_BYTES).expect("default fits")
    }
}

/// Partition and disk GUIDs, and the IFS filesystem's UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guids {
    /// Disk GUID.
    pub disk: Uuid,
    /// ESP partition GUID.
    pub esp: Uuid,
    /// LIC partition GUID.
    pub lic: Uuid,
    /// ASP1 partition GUID.
    pub asp1: Uuid,
    /// IFS partition GUID.
    pub ifs: Uuid,
    /// UUID of the XFS filesystem in the IFS partition.
    pub ifs_fs: Uuid,
}

impl Guids {
    /// Fixed GUIDs, so a built image is byte-identical between builds.
    pub const FIXED: Self = Self {
        disk: uuid!("51533430-0000-4000-8000-000000000001"),
        esp: uuid!("51533430-0000-4000-8000-000000000002"),
        lic: uuid!("51533430-0000-4000-8000-000000000003"),
        asp1: uuid!("51533430-0000-4000-8000-000000000004"),
        ifs: uuid!("51533430-0000-4000-8000-000000000005"),
        ifs_fs: uuid!("51533430-0000-4000-8000-000000000006"),
    };

    /// GUIDs from caller-supplied random bytes, for an installed disk.
    #[must_use]
    pub fn from_random(bytes: [[u8; 16]; 6]) -> Self {
        let v4 = |b: [u8; 16]| uuid::Builder::from_random_bytes(b).into_uuid();
        Self {
            disk: v4(bytes[0]),
            esp: v4(bytes[1]),
            lic: v4(bytes[2]),
            asp1: v4(bytes[3]),
            ifs: v4(bytes[4]),
            ifs_fs: v4(bytes[5]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn layout_of_the_default_image() {
        let p = Plan::default_image();
        assert_eq!(p.esp.first_lba, 2048);
        assert_eq!(p.esp.bytes(), ESP_BYTES);
        assert_eq!(p.lic.first_lba, p.esp.last_lba + 1);
        assert_eq!(p.lic.bytes(), LIC_BYTES);
        assert_eq!(p.ifs.first_lba, p.lic.last_lba + 1);
        assert_eq!(p.ifs.bytes(), 989_331_456);
        assert_eq!(p.asp1.first_lba, p.ifs.last_lba + 1);
        assert_eq!((p.asp1.last_lba + 1) % 8, 0);
        assert!(p.asp1.last_lba + TAIL_SECTORS < p.total_sectors);
        assert!(p.asp1.bytes() > 2 * GIB);
        assert!(matches!(
            Plan::for_bytes(MIN_DISK_BYTES - 1),
            Err(Error::TooSmall { .. })
        ));
        let small = Plan::for_bytes(MIN_DISK_BYTES).unwrap();
        assert_eq!(small.ifs.bytes(), MIN_IFS_BYTES);
        assert!(small.asp1.bytes() >= MIN_ASP1_BYTES - 4096);
        assert_ne!(Guids::from_random([[1; 16]; 6]).disk, Guids::FIXED.disk);
    }

    #[test]
    fn ifs_share_is_clamped() {
        assert_eq!(ifs_bytes(GIB), MIN_IFS_BYTES);
        assert_eq!(ifs_bytes(8 * GIB + 3), 2 * GIB);
        assert_eq!(ifs_bytes(4096 * GIB), MAX_IFS_BYTES);
        let big = Plan::for_bytes(2048 * GIB).unwrap();
        assert_eq!(big.ifs.bytes(), MAX_IFS_BYTES);
        assert_eq!(big.ifs.offset() % 4096, 0);
        assert_eq!(big.asp1.offset() % 4096, 0);
    }
}
