// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A disk described as an ordered list of partitions, and the one
//! writer of the protective MBR, the GPT and what each partition
//! starts with. QS/400's own layout ([`crate::Plan`]) is one such
//! description; an appliance writes another (an ESP, a record
//! partition, one data partition) without its own GPT code.
//!
//! The first partition starts 1 MiB in. Fixed partitions follow one
//! another; the one that takes the rest of the disk is last and ends on
//! a 4 KiB boundary, [`TAIL_SECTORS`] before the end, where the backup
//! GPT goes.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom, Write};

use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition::Partition;
use gpt::partition_types::{EFI, OperatingSystem, Type};
use uuid::Uuid;

use crate::plan::{ESP_FIRST_LBA, Part, SECTOR, TAIL_SECTORS};
use crate::{Error, Window};

/// Bytes zeroed at the head of a [`Content::Wiped`] partition so nothing
/// from an earlier life of the disk is mistaken for a record of ours.
pub const WIPE_BYTES: u64 = 1024 * 1024;

/// GPT partition type of an EFI System partition.
pub const ESP_TYPE_GUID: Uuid = EFI.guid;

/// How big a partition is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// This many bytes, a multiple of the sector size.
    Fixed(u64),
    /// Whatever is left, and at least `min` bytes. The last partition only.
    Rest {
        /// The least it may be.
        min: u64,
    },
}

/// What a newly written partition starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// Its sectors are left as they are.
    Keep,
    /// A volume the caller hands to the writer, exactly the partition's
    /// size (the ESP).
    Image,
    /// Its first [`WIPE_BYTES`] zeroed.
    Wiped,
    /// An empty xfs with this label (12 bytes at most) and UUID.
    Xfs {
        /// The volume label.
        label: String,
        /// The file system's UUID.
        uuid: Uuid,
    },
}

/// One partition of a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartDesc {
    /// The GPT partition name.
    pub name: String,
    /// The GPT partition type.
    pub type_guid: Uuid,
    /// How big it is.
    pub size: Size,
    /// What it starts with.
    pub content: Content,
}

/// One partition laid out on a disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    /// The GPT entry, from 1.
    pub index: u32,
    /// Its description.
    pub desc: PartDesc,
    /// Its sectors.
    pub part: Part,
}

/// A description laid out on a disk of one size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Disk size in sectors.
    pub total_sectors: u64,
    /// The partitions, in disk order.
    pub parts: Vec<Placed>,
}

/// The smallest disk `parts` fit.
pub fn min_bytes(parts: &[PartDesc]) -> u64 {
    let used: u64 = parts
        .iter()
        .map(|p| match p.size {
            Size::Fixed(b) => b,
            Size::Rest { min } => min,
        })
        .sum();
    (ESP_FIRST_LBA + TAIL_SECTORS) * SECTOR + used
}

impl Layout {
    /// Lay `parts` out on a disk of `bytes`.
    pub fn for_bytes(parts: &[PartDesc], bytes: u64) -> Result<Self, Error> {
        let min = min_bytes(parts);
        if bytes < min {
            return Err(Error::TooSmall { bytes, min });
        }
        let total_sectors = bytes / SECTOR;
        let mut next = ESP_FIRST_LBA;
        let mut placed = Vec::with_capacity(parts.len());
        for (i, desc) in parts.iter().enumerate() {
            let last_lba = match desc.size {
                Size::Fixed(b) if b == 0 || b % SECTOR != 0 => {
                    return Err(Error::Layout(format!(
                        "partition {} is {b} bytes, not a whole number of sectors",
                        desc.name
                    )));
                }
                Size::Fixed(b) => next + b / SECTOR - 1,
                Size::Rest { .. } if i + 1 != parts.len() => {
                    return Err(Error::Layout(format!(
                        "partition {} takes the rest of the disk and is not the last",
                        desc.name
                    )));
                }
                Size::Rest { .. } => (total_sectors - TAIL_SECTORS) / 8 * 8 - 1,
            };
            placed.push(Placed {
                index: u32::try_from(i + 1).unwrap_or(u32::MAX),
                desc: desc.clone(),
                part: Part {
                    first_lba: next,
                    last_lba,
                },
            });
            next = last_lba + 1;
        }
        if next + TAIL_SECTORS > total_sectors {
            return Err(Error::TooSmall { bytes, min });
        }
        Ok(Self {
            total_sectors,
            parts: placed,
        })
    }

    /// The partition named `name`.
    #[must_use]
    pub fn part(&self, name: &str) -> Option<&Placed> {
        self.parts.iter().find(|p| p.desc.name == name)
    }
}

/// The disk's GUID and one per partition, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutGuids {
    /// Disk GUID.
    pub disk: Uuid,
    /// Partition GUIDs, one per partition.
    pub parts: Vec<Uuid>,
}

impl LayoutGuids {
    /// Fixed GUIDs for `count` partitions, so a built image is
    /// byte-identical between builds: `prefix` (a product's own four
    /// bytes), then the disk as 1 and the partitions from 2.
    #[must_use]
    pub fn fixed(prefix: [u8; 4], count: usize) -> Self {
        let make = |n: u8| {
            let mut b = [0u8; 16];
            b[..4].copy_from_slice(&prefix);
            b[6] = 0x40;
            b[8] = 0x80;
            b[15] = n;
            Uuid::from_bytes(b)
        };
        Self {
            disk: make(1),
            parts: (0..count)
                .map(|i| make(u8::try_from(i + 2).unwrap_or(u8::MAX)))
                .collect(),
        }
    }
}

/// The GPT entries of `layout`.
fn entries(layout: &Layout, guids: &LayoutGuids) -> BTreeMap<u32, Partition> {
    let mut parts = BTreeMap::new();
    for (p, guid) in layout.parts.iter().zip(&guids.parts) {
        let part_type = if p.desc.type_guid == EFI.guid {
            EFI
        } else {
            Type {
                guid: p.desc.type_guid,
                os: OperatingSystem::None,
            }
        };
        parts.insert(
            p.index,
            Partition {
                part_type_guid: part_type,
                part_guid: *guid,
                first_lba: p.part.first_lba,
                last_lba: p.part.last_lba,
                flags: 0,
                name: p.desc.name.clone(),
            },
        );
    }
    parts
}

/// Write the protective MBR, the GPT and each partition's first
/// content. `images` holds the volume of every [`Content::Image`]
/// partition, by GPT entry. `dev` must already be
/// `layout.total_sectors` sectors long. `progress` is told `(bytes
/// written, bytes in all)` as it goes.
pub fn write_layout<D>(
    dev: &mut D,
    layout: &Layout,
    guids: &LayoutGuids,
    images: &[(u32, &[u8])],
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), Error>
where
    D: Read + Write + Seek + Debug,
{
    assert_eq!(
        guids.parts.len(),
        layout.parts.len(),
        "one GUID per partition"
    );
    let image = |index: u32| images.iter().find(|(i, _)| *i == index).map(|(_, b)| *b);
    for p in &layout.parts {
        if p.desc.content == Content::Image {
            let len = image(p.index).map(|b| b.len() as u64);
            assert_eq!(
                len,
                Some(p.part.bytes()),
                "the volume of partition {} matches it",
                p.desc.name
            );
        }
    }
    let mbr =
        ProtectiveMBR::with_lb_size(u32::try_from(layout.total_sectors - 1).unwrap_or(u32::MAX));
    mbr.overwrite_lba0(&mut *dev).map_err(|e| Error::Gpt {
        what: "protective MBR",
        detail: e.to_string(),
    })?;

    let mut gdisk = gpt::GptConfig::new()
        .writable(true)
        .logical_block_size(LogicalBlockSize::Lb512)
        .create_from_device(&mut *dev, Some(guids.disk))
        .map_err(|e| Error::Gpt {
            what: "header",
            detail: e.to_string(),
        })?;
    gdisk
        .update_partitions(entries(layout, guids))
        .map_err(|e| Error::Gpt {
            what: "partitions",
            detail: e.to_string(),
        })?;
    gdisk.write().map_err(|e| Error::Gpt {
        what: "write",
        detail: e.to_string(),
    })?;

    let total: u64 = layout
        .parts
        .iter()
        .map(|p| match p.desc.content {
            Content::Image => p.part.bytes(),
            Content::Wiped => WIPE_BYTES,
            Content::Keep | Content::Xfs { .. } => 0,
        })
        .sum();
    let mut written = 0u64;
    progress(0, total);
    let zeros = vec![0u8; usize::try_from(WIPE_BYTES).expect("1 MiB")];
    for p in &layout.parts {
        match &p.desc.content {
            Content::Keep => {}
            Content::Image => {
                dev.seek(SeekFrom::Start(p.part.offset()))
                    .map_err(Error::io("seek to ESP"))?;
                for chunk in image(p.index).unwrap_or_default().chunks(4 * 1024 * 1024) {
                    dev.write_all(chunk).map_err(Error::io("write ESP"))?;
                    written += chunk.len() as u64;
                    progress(written, total);
                }
            }
            Content::Wiped => {
                dev.seek(SeekFrom::Start(p.part.offset()))
                    .map_err(Error::io("seek to partition"))?;
                let n = usize::try_from(p.part.bytes().min(WIPE_BYTES)).expect("1 MiB");
                dev.write_all(&zeros[..n])
                    .map_err(Error::io("wipe partition head"))?;
                written += WIPE_BYTES;
                progress(written, total);
            }
            Content::Xfs { label, uuid } => {
                let mut opts = xfsfmt::Options::new(*uuid, xfsfmt::Timestamp { secs: 0, nsecs: 0 });
                opts.label.clone_from(label);
                let mut window = Window::new(&mut *dev, p.part.offset(), p.part.bytes());
                xfsfmt::format(&mut window, p.part.bytes(), &opts)?;
            }
        }
    }
    dev.flush().map_err(Error::io("flush"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfsfmt::SparseDev;

    const MIB: u64 = 1024 * 1024;
    const DATA: Uuid = uuid::uuid!("7d3f6c52-91aa-4e0b-b0c4-51534d430001");

    fn appliance() -> Vec<PartDesc> {
        vec![
            PartDesc {
                name: "DEMO ESP".into(),
                type_guid: ESP_TYPE_GUID,
                size: Size::Fixed(64 * MIB),
                content: Content::Image,
            },
            PartDesc {
                name: "DEMO-AB".into(),
                type_guid: uuid::uuid!("7d3f6c52-91aa-4e0b-b0c4-51534d430002"),
                size: Size::Fixed(MIB),
                content: Content::Wiped,
            },
            PartDesc {
                name: "DEMO-DATA".into(),
                type_guid: DATA,
                size: Size::Rest { min: 300 * MIB },
                content: Content::Xfs {
                    label: "DEMODATA".into(),
                    uuid: DATA,
                },
            },
        ]
    }

    #[test]
    fn an_appliance_disk_of_three_partitions() {
        let parts = appliance();
        let bytes = 512 * MIB;
        let layout = Layout::for_bytes(&parts, bytes).unwrap();
        assert_eq!(layout.parts[0].part.first_lba, 2048);
        assert_eq!(layout.parts[0].part.bytes(), 64 * MIB);
        assert_eq!(
            layout.parts[1].part.first_lba,
            layout.parts[0].part.last_lba + 1
        );
        let data = layout.part("DEMO-DATA").unwrap().clone();
        assert_eq!(data.index, 3);
        assert_eq!((data.part.last_lba + 1) % 8, 0);
        assert!(data.part.last_lba + TAIL_SECTORS < layout.total_sectors);

        let esp = vec![0x5a; usize::try_from(64 * MIB).unwrap()];
        let guids = LayoutGuids::fixed(*b"DEMO", 3);
        let mut dev = SparseDev::new(bytes);
        let mut seen = Vec::new();
        write_layout(&mut dev, &layout, &guids, &[(1, &esp)], &mut |d, t| {
            seen.push((d, t));
        })
        .unwrap();
        assert_eq!(seen.first(), Some(&(0, 65 * MIB)));
        assert_eq!(seen.last(), Some(&(65 * MIB, 65 * MIB)));
        let table = crate::read_table(&mut dev).unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(table[0].1, ESP_TYPE_GUID);
        assert_eq!(table[2], (3, DATA, data.part.first_lba, data.part.last_lba));
        assert_eq!(dev.read_at(layout.parts[0].part.offset(), 4), &[0x5a; 4]);
        // The data partition is an xfs of that label.
        let mut w = Window::new(&mut dev, data.part.offset(), data.part.bytes());
        let info = xfsfmt::probe(&mut w).unwrap().expect("an xfs");
        assert_eq!(info.label, "DEMODATA");
        // The same bytes every time.
        let mut again = SparseDev::new(bytes);
        write_layout(&mut again, &layout, &guids, &[(1, &esp)], &mut |_, _| {}).unwrap();
        assert!(again.pages().eq(dev.pages()));
        assert_ne!(guids.disk, guids.parts[0]);
    }

    #[test]
    fn descriptions_that_cannot_be_laid_out() {
        let parts = appliance();
        assert!(matches!(
            Layout::for_bytes(&parts, min_bytes(&parts) - 1),
            Err(Error::TooSmall { .. })
        ));
        Layout::for_bytes(&parts, min_bytes(&parts)).unwrap();
        let mut rest_first = parts.clone();
        rest_first.swap(0, 2);
        assert!(matches!(
            Layout::for_bytes(&rest_first, 1024 * MIB),
            Err(Error::Layout(_))
        ));
        let mut odd = parts;
        odd[1].size = Size::Fixed(1000);
        assert!(matches!(
            Layout::for_bytes(&odd, 1024 * MIB),
            Err(Error::Layout(_))
        ));
    }
}
