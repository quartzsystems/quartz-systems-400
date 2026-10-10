// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! GPT disk image: a FAT32 ESP holding slot A and slot B, then the raw
//! LIC partition.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition::Partition;
use gpt::partition_types::{EFI, OperatingSystem, Type};
use tracing::info;
use uuid::{Uuid, uuid};

use crate::ctx::Ctx;
use crate::host::hash::{sha256_file, short};
use crate::host::paths;
use crate::image::esp;
use crate::kernel::UkiKind;

const SECTOR: u64 = 512;
const ESP_FIRST_LBA: u64 = 2048;
const ESP_BYTES: u64 = esp::DISK_ESP.bytes;
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

/// `build disk`.
pub fn disk(ctx: &Ctx) -> Result<PathBuf> {
    let uki_a = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))
        .context("no slot A UKI; run `uki` first")?;
    let uki_b = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotB))
        .context("no slot B UKI; run `uki` first")?;
    let a_bytes = fs::read(&uki_a)?;
    let b_bytes = fs::read(&uki_b)?;
    let hash = sha256_file(&uki_a)?;

    info!(
        "disk: formatting {} MiB FAT32 ESP",
        ESP_BYTES / (1024 * 1024)
    );
    let esp = esp::build(
        &esp::DISK_ESP,
        &[
            (&format!("EFI/BOOT/{SLOT_A}"), &a_bytes),
            (&format!("EFI/QS400/{SLOT_B}"), &b_bytes),
        ],
    )?;
    let listing = esp::list(&esp)?;
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
        .join(format!("qs400-{}-{}.img", release::RELEASE, short(&hash)));
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
