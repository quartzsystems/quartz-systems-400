// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Giving a disk laid out before the IFS partition existed one: in the
//! unused space after its last partition (a disk that has grown). The
//! existing partitions keep their entries, GUIDs and sectors; only the
//! table is rewritten, after the new filesystem is in place. Shrinking
//! ASP1 to make room is not done here.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{Read, Seek, Write};

use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition::Partition;
use gpt::partition_types::{OperatingSystem, Type};

use crate::Error;
use crate::plan::{
    IFS_INDEX, IFS_NAME, IFS_TYPE_GUID, MAX_IFS_BYTES, MIN_IFS_BYTES, Part, SECTOR, TAIL_SECTORS,
};
use crate::table::IfsFormat;
use crate::window::Window;

/// Partition alignment for a new IFS: 1 MiB.
const ALIGN_SECTORS: u64 = 2048;

/// What [`add_ifs`] found or did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddIfs {
    /// The disk already has an IFS partition.
    Present,
    /// A new IFS partition, formatted, in GPT entry `index`.
    Added {
        /// Its sectors.
        part: Part,
        /// Its GPT entry.
        index: u32,
    },
    /// Too little unused space after the last partition.
    NoSpace {
        /// Unused bytes found.
        free: u64,
    },
}

/// Add an IFS partition to the disk `dev` of `bytes` when it has none
/// and the space after its last partition holds one. `part_guid` names
/// the partition, `fs_uuid` its filesystem.
pub fn add_ifs<D>(
    dev: &mut D,
    bytes: u64,
    part_guid: uuid::Uuid,
    fs_uuid: uuid::Uuid,
    ifs: &IfsFormat,
) -> Result<AddIfs, Error>
where
    D: Read + Write + Seek + Debug,
{
    let gpt_err = |what| {
        move |e: gpt::GptError| Error::Gpt {
            what,
            detail: e.to_string(),
        }
    };
    let (disk_guid, old) = {
        let disk = gpt::GptConfig::new()
            .writable(false)
            .logical_block_size(LogicalBlockSize::Lb512)
            .open_from_device(&mut *dev)
            .map_err(gpt_err("read"))?;
        (disk.guid().to_owned(), disk.partitions().clone())
    };
    if old.values().any(|p| p.part_type_guid.guid == IFS_TYPE_GUID) {
        return Ok(AddIfs::Present);
    }
    let total_sectors = bytes / SECTOR;
    let used_end = old
        .values()
        .map(|p| p.last_lba + 1)
        .max()
        .unwrap_or(ALIGN_SECTORS);
    let first = used_end.next_multiple_of(ALIGN_SECTORS);
    let end = (total_sectors.saturating_sub(TAIL_SECTORS)) / 8 * 8;
    let free = end.saturating_sub(first) * SECTOR;
    let size = free.min(MAX_IFS_BYTES) / 4096 * 4096;
    if size < MIN_IFS_BYTES {
        return Ok(AddIfs::NoSpace { free });
    }
    let part = Part {
        first_lba: first,
        last_lba: first + size / SECTOR - 1,
    };
    let index = (IFS_INDEX..=128)
        .find(|i| !old.contains_key(i))
        .ok_or(Error::Gpt {
            what: "partitions",
            detail: "no free entry".into(),
        })?;

    // The filesystem first: until the table names it, it is unused space.
    let fs_opts = xfsfmt::Options {
        label: IFS_NAME.to_string(),
        zeroed: ifs.zeroed,
        ..xfsfmt::Options::new(fs_uuid, ifs.time)
    };
    xfsfmt::format(
        &mut Window::new(&mut *dev, part.offset(), size),
        size,
        &fs_opts,
    )?;

    // Then the table, for the disk's whole present size.
    let mbr = ProtectiveMBR::with_lb_size(u32::try_from(total_sectors - 1).unwrap_or(u32::MAX));
    mbr.overwrite_lba0(&mut *dev).map_err(|e| Error::Gpt {
        what: "protective MBR",
        detail: e.to_string(),
    })?;
    let mut disk = gpt::GptConfig::new()
        .writable(true)
        .logical_block_size(LogicalBlockSize::Lb512)
        .create_from_device(&mut *dev, Some(disk_guid))
        .map_err(gpt_err("header"))?;
    let mut parts: BTreeMap<u32, Partition> = old;
    parts.insert(
        index,
        Partition {
            part_type_guid: Type {
                guid: IFS_TYPE_GUID,
                os: OperatingSystem::None,
            },
            part_guid,
            first_lba: part.first_lba,
            last_lba: part.last_lba,
            flags: 0,
            name: IFS_NAME.to_string(),
        },
    );
    disk.update_partitions(parts)
        .map_err(gpt_err("partitions"))?;
    disk.write().map_err(gpt_err("write"))?;
    dev.flush().map_err(Error::io("flush"))?;
    Ok(AddIfs::Added { part, index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ASP1_TYPE_GUID, Guids, LIC_TYPE_GUID, Plan, fat, read_table, write_image};
    use std::io::SeekFrom;
    use xfsfmt::{SparseDev, Timestamp};

    const GIB: u64 = 1024 * 1024 * 1024;
    const IFS: IfsFormat = IfsFormat {
        time: Timestamp {
            secs: 1_767_225_600,
            nsecs: 0,
        },
        zeroed: true,
    };

    /// A disk as written before the IFS: ESP, LIC, ASP1 to the end.
    fn old_disk(bytes: u64) -> (SparseDev, Plan) {
        let plan = Plan::for_bytes(bytes).unwrap();
        let mut dev = SparseDev::new(bytes);
        let esp = fat::build(&fat::DISK_ESP, &[]).unwrap();
        write_image(&mut dev, &plan, &Guids::FIXED, &esp, &IFS).unwrap();
        let mut disk = gpt::GptConfig::new()
            .writable(true)
            .logical_block_size(LogicalBlockSize::Lb512)
            .open_from_device(&mut dev)
            .unwrap();
        let mut parts = disk.partitions().clone();
        parts.remove(&IFS_INDEX);
        parts.get_mut(&3).unwrap().first_lba = plan.ifs.first_lba;
        disk.update_partitions(parts).unwrap();
        disk.write().unwrap();
        // Forget the formatted IFS: an old disk had ASP1 there.
        dev.seek(SeekFrom::Start(plan.ifs.offset())).unwrap();
        dev.write_all(&[0; 4096]).unwrap();
        (dev, plan)
    }

    /// The same disk after it has grown by `more` bytes.
    fn grown(old: &SparseDev, more: u64) -> SparseDev {
        let mut dev = SparseDev::new(old.len() + more);
        for (at, page) in old.pages() {
            dev.seek(SeekFrom::Start(at)).unwrap();
            dev.write_all(page).unwrap();
        }
        dev
    }

    #[test]
    fn a_full_disk_has_no_room() {
        let (mut dev, _) = old_disk(2 * GIB);
        let r = add_ifs(
            &mut dev,
            2 * GIB,
            Guids::FIXED.ifs,
            Guids::FIXED.ifs_fs,
            &IFS,
        )
        .unwrap();
        assert!(matches!(r, AddIfs::NoSpace { free } if free < MIN_IFS_BYTES));
        assert_eq!(read_table(&mut dev).unwrap().len(), 3);
    }

    #[test]
    fn a_grown_disk_gets_the_ifs_at_its_tail() {
        let (old, plan) = old_disk(2 * GIB);
        let mut dev = grown(&old, GIB);
        let r = add_ifs(
            &mut dev,
            3 * GIB,
            Guids::FIXED.ifs,
            Guids::FIXED.ifs_fs,
            &IFS,
        )
        .unwrap();
        let AddIfs::Added { part, index } = r else {
            panic!("{r:?}");
        };
        assert_eq!(index, IFS_INDEX);
        assert_eq!(part.first_lba % ALIGN_SECTORS, 0);
        assert!(part.first_lba > plan.asp1.last_lba);
        assert!(part.bytes() >= MIN_IFS_BYTES);
        let table = read_table(&mut dev).unwrap();
        assert_eq!(table.len(), 4);
        assert_eq!(table[1].1, LIC_TYPE_GUID);
        assert_eq!(
            table[2],
            (3, ASP1_TYPE_GUID, plan.ifs.first_lba, plan.asp1.last_lba)
        );
        assert_eq!(table[3], (4, IFS_TYPE_GUID, part.first_lba, part.last_lba));
        let mut fs = Window::new(&mut dev, part.offset(), part.bytes());
        assert!(xfsfmt::probe(&mut fs).unwrap().is_some());
        // Once is enough.
        let again = add_ifs(
            &mut dev,
            3 * GIB,
            Guids::FIXED.ifs,
            Guids::FIXED.ifs_fs,
            &IFS,
        );
        assert_eq!(again.unwrap(), AddIfs::Present);
    }
}
