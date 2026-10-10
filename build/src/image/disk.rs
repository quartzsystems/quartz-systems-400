// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! GPT disk image: a FAT32 ESP holding slot A and slot B, the raw LIC
//! partition, and the ASP1 partition filling the rest. The layout itself
//! lives in `diskimg`, which the installer shares.

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use diskimg::{Guids, Plan, fat};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::{sha256_file, short};
use crate::host::paths;
use crate::kernel::UkiKind;

/// Slot A: the removable-media default boot path.
const SLOT_A: &str = "BOOTX64.EFI";
/// Slot B.
const SLOT_B: &str = "B.EFI";

/// `build disk`: an image of `bytes` (the 4 GiB default when `None`).
pub fn disk(ctx: &Ctx, bytes: Option<u64>) -> Result<PathBuf> {
    let uki_a = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))
        .context("no slot A UKI; run `uki` first")?;
    let uki_b = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotB))
        .context("no slot B UKI; run `uki` first")?;
    let a_bytes = fs::read(&uki_a)?;
    let b_bytes = fs::read(&uki_b)?;
    let hash = sha256_file(&uki_a)?;
    let plan = Plan::for_bytes(bytes.unwrap_or(diskimg::DEFAULT_DISK_BYTES))?;

    info!(
        "disk: formatting {} MiB FAT32 ESP",
        plan.esp.bytes() / (1024 * 1024)
    );
    let esp = fat::build(
        &fat::DISK_ESP,
        &[
            (&format!("EFI/BOOT/{SLOT_A}"), &a_bytes),
            (&format!("EFI/QS400/{SLOT_B}"), &b_bytes),
        ],
    )?;
    let listing = fat::list(&esp)?;
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
    fs::create_dir_all(out.parent().context("no parent")?)?;
    let mut file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&out)
        .with_context(|| format!("cannot create {}", out.display()))?;
    file.set_len(plan.total_sectors * diskimg::SECTOR)?;
    diskimg::write_image(&mut file, &plan, &Guids::FIXED, &esp)?;
    file.sync_all()?;
    paths::write_pointer(&ctx.paths.disk_pointer(), &out)?;
    info!(
        "disk: {} ({} bytes: ESP {} MiB, LIC {} MiB, ASP1 {} MiB; sha256 {})",
        out.display(),
        fs::metadata(&out)?.len(),
        plan.esp.bytes() >> 20,
        plan.lic.bytes() >> 20,
        plan.asp1.bytes() >> 20,
        sha256_file(&out)?
    );
    Ok(out)
}
