// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing the partition table and the ESP onto a disk or image.

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{Read, Seek, SeekFrom, Write};

use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition::Partition;
use gpt::partition_types::{EFI, OperatingSystem, Type};

use crate::plan::{ASP1_NAME, ASP1_TYPE_GUID, ESP_NAME, LIC_NAME, LIC_TYPE_GUID};
use crate::{Error, Guids, Plan};

/// Bytes zeroed at the head of the LIC and ASP1 partitions so nothing
/// from an earlier life of the disk is mistaken for a QS/400 record.
pub const WIPE_BYTES: u64 = 1024 * 1024;

/// Write the protective MBR, the GPT, the ESP volume `esp`, and zero
/// the heads of the LIC and ASP1 partitions. `dev` must already be
/// `plan.total_sectors` sectors long.
pub fn write_image<D>(dev: &mut D, plan: &Plan, guids: &Guids, esp: &[u8]) -> Result<(), Error>
where
    D: Read + Write + Seek + Debug,
{
    assert_eq!(
        esp.len() as u64,
        plan.esp.bytes(),
        "ESP volume matches its partition"
    );
    let mbr =
        ProtectiveMBR::with_lb_size(u32::try_from(plan.total_sectors - 1).unwrap_or(u32::MAX));
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
    let mut parts = BTreeMap::new();
    let part = |type_guid, guid, p: crate::Part, name: &str| Partition {
        part_type_guid: type_guid,
        part_guid: guid,
        first_lba: p.first_lba,
        last_lba: p.last_lba,
        flags: 0,
        name: name.to_string(),
    };
    let quartz = |guid| Type {
        guid,
        os: OperatingSystem::None,
    };
    parts.insert(1, part(EFI, guids.esp, plan.esp, ESP_NAME));
    parts.insert(
        2,
        part(quartz(LIC_TYPE_GUID), guids.lic, plan.lic, LIC_NAME),
    );
    parts.insert(
        3,
        part(quartz(ASP1_TYPE_GUID), guids.asp1, plan.asp1, ASP1_NAME),
    );
    gdisk.update_partitions(parts).map_err(|e| Error::Gpt {
        what: "partitions",
        detail: e.to_string(),
    })?;
    gdisk.write().map_err(|e| Error::Gpt {
        what: "write",
        detail: e.to_string(),
    })?;

    dev.seek(SeekFrom::Start(plan.esp.offset()))
        .map_err(Error::io("seek to ESP"))?;
    dev.write_all(esp).map_err(Error::io("write ESP"))?;
    let zeros = vec![0u8; usize::try_from(WIPE_BYTES).expect("1 MiB")];
    for p in [plan.lic, plan.asp1] {
        dev.seek(SeekFrom::Start(p.offset()))
            .map_err(Error::io("seek to partition"))?;
        dev.write_all(&zeros)
            .map_err(Error::io("wipe partition head"))?;
    }
    dev.flush().map_err(Error::io("flush"))?;
    Ok(())
}

/// Read back the partition table of a device written by `write_image`:
/// `(index, type GUID, first LBA, last LBA)` per partition.
pub fn read_table<D>(dev: &mut D) -> Result<Vec<(u32, uuid::Uuid, u64, u64)>, Error>
where
    D: Read + Write + Seek + Debug,
{
    let gdisk = gpt::GptConfig::new()
        .writable(false)
        .logical_block_size(LogicalBlockSize::Lb512)
        .open_from_device(&mut *dev)
        .map_err(|e| Error::Gpt {
            what: "read",
            detail: e.to_string(),
        })?;
    Ok(gdisk
        .partitions()
        .iter()
        .map(|(i, p)| (*i, p.part_type_guid.guid, p.first_lba, p.last_lba))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fat;
    use crate::plan::SECTOR;
    use std::io::Cursor;

    #[test]
    fn image_round_trip() {
        let plan = Plan::for_bytes(crate::MIN_DISK_BYTES).unwrap();
        let mut dev = Cursor::new(vec![
            0u8;
            usize::try_from(plan.total_sectors * SECTOR).unwrap()
        ]);
        let esp = fat::build(&fat::DISK_ESP, &[("EFI/BOOT/BOOTX64.EFI", b"uki")]).unwrap();
        write_image(&mut dev, &plan, &Guids::FIXED, &esp).unwrap();
        let table = read_table(&mut dev).unwrap();
        assert_eq!(table.len(), 3);
        assert_eq!(
            table[0],
            (1, EFI.guid, plan.esp.first_lba, plan.esp.last_lba)
        );
        assert_eq!(table[1].1, LIC_TYPE_GUID);
        assert_eq!(
            table[2],
            (3, ASP1_TYPE_GUID, plan.asp1.first_lba, plan.asp1.last_lba)
        );
        let bytes = dev.into_inner();
        let at = usize::try_from(plan.esp.offset()).unwrap();
        assert_eq!(&bytes[at..at + 512], &esp[..512]);
        // Writing twice gives the same bytes.
        let mut again = Cursor::new(vec![0u8; bytes.len()]);
        write_image(&mut again, &plan, &Guids::FIXED, &esp).unwrap();
        assert_eq!(again.into_inner(), bytes);
    }
}
