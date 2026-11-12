// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The GPT disk image: the partitions the product describes
//! (`diskimg::layout`), the first of them an ESP holding slot A at
//! `EFI/BOOT/BOOTX64.EFI` and slot B at `EFI/<VENDOR>/B.EFI`.

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use diskimg::fat;
use diskimg::layout::{Content, ESP_TYPE_GUID, Layout, write_layout};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::{sha256_file, short};
use crate::host::paths;
use crate::kernel::UkiKind;
use crate::product::Product;

/// Slot A on the ESP: the removable-media default boot path.
pub const SLOT_A: &str = "EFI/BOOT/BOOTX64.EFI";

/// Slot B on the ESP: `EFI/<VENDOR>/B.EFI`.
pub fn slot_b(product: &Product) -> String {
    format!("EFI/{}/B.EFI", product.vendor)
}

/// The ESP volume: both slots' UKIs, and nothing else.
pub fn esp(ctx: &Ctx, bytes: u64) -> Result<Vec<u8>> {
    let uki_a = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))
        .context("no slot A UKI; run `uki` first")?;
    let uki_b = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotB))
        .context("no slot B UKI; run `uki` first")?;
    let a_bytes = fs::read(&uki_a)?;
    let b_bytes = fs::read(&uki_b)?;
    let slot_b = slot_b(&ctx.product);
    let esp = fat::build(
        &fat::esp_spec(bytes),
        &[(SLOT_A, &a_bytes), (slot_b.as_str(), &b_bytes)],
    )?;
    let listing = fat::list(&esp)?;
    let expected = [
        "EFI/".to_string(),
        "EFI/BOOT/".to_string(),
        SLOT_A.to_string(),
        format!("EFI/{}/", ctx.product.vendor),
        slot_b,
    ];
    ensure!(listing == expected, "ESP contents unexpected: {listing:?}");
    Ok(esp)
}

/// `build disk`: an image of `bytes` (the product's default when `None`).
pub fn disk(ctx: &Ctx, bytes: Option<u64>) -> Result<PathBuf> {
    let uki_a = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))
        .context("no slot A UKI; run `uki` first")?;
    let hash = sha256_file(&uki_a)?;

    let parts = (ctx.product.disk_parts)();
    let layout = Layout::for_bytes(&parts, bytes.unwrap_or(ctx.product.disk_bytes))?;
    let esp_part = layout
        .parts
        .iter()
        .find(|p| p.desc.type_guid == ESP_TYPE_GUID && p.desc.content == Content::Image)
        .context("the product's disk has no ESP")?;
    info!(
        "disk: formatting {} MiB FAT ESP",
        esp_part.part.bytes() / (1024 * 1024)
    );
    let esp = esp(ctx, esp_part.part.bytes())?;

    let out = ctx.paths.images().join(format!(
        "{}-{}-{}.img",
        ctx.product.id,
        ctx.product.release,
        short(&hash)
    ));
    fs::create_dir_all(out.parent().context("no parent")?)?;
    let mut file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&out)
        .with_context(|| format!("cannot create {}", out.display()))?;
    file.set_len(layout.total_sectors * diskimg::SECTOR)?;
    let guids = (ctx.product.disk_guids)();
    write_layout(
        &mut file,
        &layout,
        &guids,
        &[(esp_part.index, &esp)],
        &mut |_, _| {},
    )?;
    file.sync_all()?;
    paths::write_pointer(&ctx.paths.disk_pointer(), &out)?;

    let sizes: Vec<String> = layout
        .parts
        .iter()
        .map(|p| format!("{} {} MiB", p.desc.name, p.part.bytes() >> 20))
        .collect();
    info!(
        "disk: {} ({} bytes: {}; sha256 {})",
        out.display(),
        fs::metadata(&out)?.len(),
        sizes.join(", "),
        sha256_file(&out)?
    );
    Ok(out)
}
