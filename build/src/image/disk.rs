// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! GPT disk image: a FAT32 ESP holding slot A and slot B, then the raw
//! LIC partition.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use fatfs::{Dir, FatType, FileSystem, FormatVolumeOptions, FsOptions, ReadWriteSeek};
use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition::Partition;
use gpt::partition_types::{EFI, OperatingSystem, Type};
use tracing::info;
use uuid::{Uuid, uuid};

use crate::ctx::Ctx;
use crate::host::hash::{sha256_file, short};
use crate::host::paths;
use crate::kernel::fetch;

const SECTOR: u64 = 512;
const ESP_FIRST_LBA: u64 = 2048;
const ESP_BYTES: u64 = 256 * 1024 * 1024;
const ESP_SECTORS: u64 = ESP_BYTES / SECTOR;
/// Raw LIC partition right after the ESP. Zero-filled; slic initializes it
/// on first boot.
const LIC_FIRST_LBA: u64 = ESP_FIRST_LBA + ESP_SECTORS;
const LIC_BYTES: u64 = 64 * 1024 * 1024;
const LIC_SECTORS: u64 = LIC_BYTES / SECTOR;
/// Room for the backup GPT after the last partition.
const TAIL_SECTORS: u64 = 2048;
const TOTAL_SECTORS: u64 = LIC_FIRST_LBA + LIC_SECTORS + TAIL_SECTORS;

/// Slot A: the removable-media default boot path.
const SLOT_A: &str = "BOOTX64.EFI";
/// Slot B.
const SLOT_B: &str = "B.EFI";

/// Fixed GUIDs keep the image byte-identical between builds.
const DISK_GUID: Uuid = uuid!("51533430-0000-4000-8000-000000000001");
const ESP_GUID: Uuid = uuid!("51533430-0000-4000-8000-000000000002");
const LIC_GUID: Uuid = uuid!("51533430-0000-4000-8000-000000000003");
const FAT_VOLUME_ID: u32 = 0x5153_3430;
const FAT_LABEL: [u8; 11] = *b"QS400 ESP  ";

/// `build disk`.
pub fn disk(ctx: &Ctx) -> Result<PathBuf> {
    let uki = paths::read_pointer(&ctx.paths.uki_pointer()).context("no UKI; run `uki` first")?;
    let uki_bytes = fs::read(&uki)?;
    let version = fetch::read_pin(&ctx.paths)?.version;
    let hash = sha256_file(&uki)?;

    info!("disk: formatting {} MiB ESP", ESP_BYTES / (1024 * 1024));
    let esp = build_esp(&uki_bytes)?;
    let listing = list_esp(&esp)?;
    let expected = [
        "EFI/".to_string(),
        "EFI/BOOT/".to_string(),
        format!("EFI/BOOT/{SLOT_A}"),
        "EFI/QS400/".to_string(),
        format!("EFI/QS400/{SLOT_B}"),
    ];
    ensure!(listing == expected, "ESP contents unexpected: {listing:?}");

    let out = ctx
        .paths
        .images()
        .join(format!("qs400-{version}-{}.img", short(&hash)));
    write_image(&out, &esp)?;
    paths::write_pointer(&ctx.paths.disk_pointer(), &out)?;
    info!(
        "disk: {} ({} bytes, sha256 {})",
        out.display(),
        fs::metadata(&out)?.len(),
        sha256_file(&out)?
    );
    Ok(out)
}

/// Format a FAT32 volume in memory and place both slots.
fn build_esp(uki: &[u8]) -> Result<Vec<u8>> {
    let size = usize::try_from(ESP_BYTES).context("ESP size")?;
    let mut cur = Cursor::new(vec![0u8; size]);
    // fatfs picks the FAT type from the cluster count: 256 MiB at 2 KiB per
    // cluster is 131072 clusters, which forces FAT32.
    fatfs::format_volume(
        &mut cur,
        FormatVolumeOptions::new()
            .fat_type(FatType::Fat32)
            .bytes_per_cluster(2048)
            .volume_id(FAT_VOLUME_ID)
            .volume_label(FAT_LABEL),
    )
    .context("FAT32 format")?;
    let fs = FileSystem::new(&mut cur, FsOptions::new()).context("FAT mount")?;
    ensure!(
        fs.fat_type() == FatType::Fat32,
        "ESP formatted as {:?}, not FAT32",
        fs.fat_type()
    );
    {
        let efi = fs.root_dir().create_dir("EFI")?;
        let boot = efi.create_dir("BOOT")?;
        let mut a = boot.create_file(SLOT_A)?;
        a.write_all(uki)?;
        a.flush()?;
        drop(a);
        let qs = efi.create_dir("QS400")?;
        let mut b = qs.create_file(SLOT_B)?;
        b.write_all(uki)?;
        b.flush()?;
    }
    fs.unmount()?;
    Ok(cur.into_inner())
}

/// Every path on the volume, directories with a trailing slash, sorted.
fn list_esp(esp: &[u8]) -> Result<Vec<String>> {
    let mut cur = Cursor::new(esp.to_vec());
    let fs = FileSystem::new(&mut cur, FsOptions::new()).context("FAT re-mount")?;
    let mut out = Vec::new();
    walk(&fs.root_dir(), "", &mut out)?;
    out.sort();
    Ok(out)
}

fn walk<T: ReadWriteSeek>(dir: &Dir<'_, T>, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for entry in dir.iter() {
        let entry = entry?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = format!("{prefix}{name}");
        if entry.is_dir() {
            out.push(format!("{path}/"));
            walk(&entry.to_dir(), &format!("{path}/"), out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Protective MBR, GPT with one ESP, then the volume bytes.
fn write_image(out: &PathBuf, esp: &[u8]) -> Result<()> {
    fs::create_dir_all(out.parent().context("no parent")?)?;
    let mut file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(out)
        .with_context(|| format!("cannot create {}", out.display()))?;
    file.set_len(TOTAL_SECTORS * SECTOR)?;

    let mbr = ProtectiveMBR::with_lb_size(u32::try_from(TOTAL_SECTORS - 1).unwrap_or(u32::MAX));
    mbr.overwrite_lba0(&mut file).context("protective MBR")?;

    let mut gdisk = gpt::GptConfig::new()
        .writable(true)
        .logical_block_size(LogicalBlockSize::Lb512)
        .create_from_device(file, Some(DISK_GUID))
        .context("GPT header")?;
    let mut parts = BTreeMap::new();
    parts.insert(
        1,
        Partition {
            part_type_guid: EFI,
            part_guid: ESP_GUID,
            first_lba: ESP_FIRST_LBA,
            last_lba: ESP_FIRST_LBA + ESP_SECTORS - 1,
            flags: 0,
            name: "QS400 ESP".to_string(),
        },
    );
    parts.insert(
        2,
        Partition {
            part_type_guid: Type {
                guid: licpart::LIC_TYPE_GUID,
                os: OperatingSystem::None,
            },
            part_guid: LIC_GUID,
            first_lba: LIC_FIRST_LBA,
            last_lba: LIC_FIRST_LBA + LIC_SECTORS - 1,
            flags: 0,
            name: licpart::PARTITION_NAME.to_string(),
        },
    );
    gdisk.update_partitions(parts).context("GPT partitions")?;
    let mut file = gdisk.write().context("GPT write")?;

    file.seek(SeekFrom::Start(ESP_FIRST_LBA * SECTOR))?;
    file.write_all(esp)?;
    file.sync_all()?;
    Ok(())
}
